/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _GNU_SOURCE
#include "../include/pocket.h"
#include <linux/fb.h>
#include <linux/input.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <fcntl.h>
#include <dirent.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <errno.h>
static uint32_t canvas[POCKET_PIXELS];
static PocketFrame *game;
static int game_fd=-1,overlay=0,stop=0,headless=0;
static pid_t pid=-1;
static char game_bin[512]="pocket-game",sock_path[108]=POCKET_SOCK;
static int server=-1,fbfd=-1,input[24],input_count=0;
static uint8_t *fbmem;static size_t fbsize;static struct fb_var_screeninfo var;static struct fb_fix_screeninfo fix;
static volatile sig_atomic_t sigstop=0;
static void on_signal(int n){(void)n;sigstop=1;}
static void ms(int n){struct timespec t={n/1000,(long)(n%1000)*1000000L};nanosleep(&t,0);}
static void rect(int x,int y,int w,int h,uint32_t color){
 if(x<0){w+=x;x=0;}if(y<0){h+=y;y=0;}
 if(x+w>POCKET_W)w=POCKET_W-x;if(y+h>POCKET_H)h=POCKET_H-y;
 if(w<=0||h<=0)return;
 for(int j=y;j<y+h;j++)for(int i=x;i<x+w;i++)canvas[j*POCKET_W+i]=color;
}
static void render(void){
 for(int i=0;i<POCKET_PIXELS;i++)canvas[i]=0x111827;
 if(pid>0&&game){
  memcpy(canvas,game->pixels,sizeof(canvas));
  rect(0,0,POCKET_W,22,0x182433);
 }else{
  rect(24,30,POCKET_W-48,54,0x233448);
  rect(24,100,POCKET_W-48,48,0x345b68);
  rect(24,160,POCKET_W-48,48,0x243548);
 }
 if(overlay){
  rect(0,0,POCKET_W,POCKET_H,0x0a1422);
  if(pid>0&&game)for(int y=0;y<POCKET_H;y++)for(int x=0;x<POCKET_W*2/3;x++)
   canvas[y*POCKET_W+x]=game->pixels[y*POCKET_W+x];
  rect(POCKET_W*2/3,0,POCKET_W/3,POCKET_H,0x283e51);
  rect(POCKET_W*2/3,0,3,POCKET_H,0x72dfca);
  for(int j=0;j<3;j++)rect(POCKET_W*2/3+16,30+j*64,POCKET_W/3-32,44,0x3d596d);
 }
}
static uint32_t bitfield(unsigned char c,struct fb_bitfield f){
 if(!f.length)return 0;
 uint32_t mask=(1u<<f.length)-1;
 return ((((uint32_t)c*mask+127)/255)<<f.offset);
}
static int init_fb(void){
 fbfd=open("/dev/fb0",O_RDWR);if(fbfd<0)return -1;
 if(ioctl(fbfd,FBIOGET_VSCREENINFO,&var)<0||ioctl(fbfd,FBIOGET_FSCREENINFO,&fix)<0)return -1;
 if(var.bits_per_pixel!=16&&var.bits_per_pixel!=24&&var.bits_per_pixel!=32)return -1;
 fbsize=fix.smem_len;fbmem=mmap(0,fbsize,PROT_READ|PROT_WRITE,MAP_SHARED,fbfd,0);
 return fbmem==MAP_FAILED?-1:0;
}
static void present(void){
 if(!fbmem)return;
 int w=(int)var.xres,h=(int)var.yres;
 for(int y=0;y<h;y++){
  uint8_t *row=fbmem+(size_t)(y+var.yoffset)*fix.line_length;
  for(int x=0;x<w;x++){
   int sx=x*POCKET_W/w,sy=y*POCKET_H/h;
   uint32_t c=canvas[sy*POCKET_W+sx];
   uint32_t p=bitfield((c>>16)&255,var.red)|bitfield((c>>8)&255,var.green)|bitfield(c&255,var.blue);
   uint8_t *q=row+(size_t)(x+var.xoffset)*(var.bits_per_pixel/8);
   q[0]=(unsigned char)p;q[1]=(unsigned char)(p>>8);
   if(var.bits_per_pixel>=24)q[2]=(unsigned char)(p>>16);
   if(var.bits_per_pixel==32)q[3]=(unsigned char)(p>>24);
  }
 }
}
static void shot(const char *path){
 FILE *f=fopen(path,"wb");if(!f){perror(path);return;}
 fprintf(f,"P6\n%d %d\n255\n",POCKET_W,POCKET_H);
 for(int i=0;i<POCKET_PIXELS;i++){uint32_t c=canvas[i];unsigned char b[3]={(c>>16)&255,(c>>8)&255,c&255};fwrite(b,1,3,f);}fclose(f);
}
static void game_stop(void){
 if(game)atomic_store(&game->alive,0);
 if(pid>0){int status;for(int t=0;t<40;t++){if(waitpid(pid,&status,WNOHANG)==pid){pid=-1;break;}ms(10);}
  if(pid>0){kill(pid,SIGTERM);waitpid(pid,&status,0);pid=-1;}}
 if(game){munmap(game,sizeof(*game));game=0;}
 if(game_fd>=0){close(game_fd);game_fd=-1;}
 overlay=0;
}
static int game_start(void){
 if(pid>0)return 0;
 game_fd=memfd_create("pokshell-game",0);if(game_fd<0)return -1;
 if(ftruncate(game_fd,sizeof(PocketFrame))<0){close(game_fd);game_fd=-1;return -1;}
 game=mmap(0,sizeof(PocketFrame),PROT_READ|PROT_WRITE,MAP_SHARED,game_fd,0);
 if(game==MAP_FAILED){game=0;close(game_fd);game_fd=-1;return -1;}
 memset(game,0,sizeof(*game));atomic_store(&game->alive,1);
 pid=fork();if(pid<0){pid=-1;game_stop();return -1;}
 if(pid==0){char fd[20];snprintf(fd,sizeof(fd),"%d",game_fd);setenv("POCKET_GAME_FD",fd,1);
  execl(game_bin,game_bin,(char*)0);_exit(127);}
 return 0;
}
static int init_socket(void){
 server=socket(AF_UNIX,SOCK_STREAM|SOCK_NONBLOCK,0);if(server<0)return -1;
 struct sockaddr_un a={.sun_family=AF_UNIX};if(strlen(sock_path)>=sizeof(a.sun_path))return -1;
 strcpy(a.sun_path,sock_path);unlink(sock_path);
 if(bind(server,(void*)&a,sizeof(a))<0||listen(server,8)<0)return -1;
 chmod(sock_path,0600);return 0;
}
static void command(void){
 for(;;){int fd=accept4(server,0,0,SOCK_NONBLOCK);if(fd<0)break;
 char m[128];ssize_t n=read(fd,m,sizeof(m)-1);
 if(n<=0){close(fd);continue;}m[n]=0;
 if(strncmp(m,"STATUS",6)==0)snprintf(m,sizeof(m),"mode=%s overlay=%d pid=%ld\n",pid>0?"game":"home",overlay,(long)pid);
 else if(strncmp(m,"GAME START",10)==0)snprintf(m,sizeof(m),"%s\n",game_start()==0?"OK GAME START":"ERROR GAME START");
 else if(strncmp(m,"GAME STOP",9)==0){game_stop();strcpy(m,"OK GAME STOP\n");}
 else if(strncmp(m,"OVERLAY OPEN",12)==0){overlay=1;strcpy(m,"OK OVERLAY OPEN\n");}
 else if(strncmp(m,"OVERLAY CLOSE",13)==0){overlay=0;strcpy(m,"OK OVERLAY CLOSED\n");}
 else if(strncmp(m,"QUIT",4)==0){stop=1;strcpy(m,"OK QUIT\n");}
 else strcpy(m,"ERROR UNKNOWN COMMAND\n");
 (void)write(fd,m,strlen(m));close(fd);
 }
}
static void input_scan(void){
 DIR *d=opendir("/dev/input");if(!d)return;
 struct dirent *e;while((e=readdir(d))!=0){
  if(strncmp(e->d_name,"event",5)!=0)continue;
  char path[128];snprintf(path,sizeof(path),"/dev/input/%.90s",e->d_name);
  int fd=open(path,O_RDONLY|O_NONBLOCK);
  if(fd<0)continue;
  if(input_count<24)input[input_count++]=fd;else close(fd);
 }closedir(d);
}
static void input_poll(void){
 for(int i=0;i<input_count;i++){struct input_event e;
  while(read(input[i],&e,sizeof(e))==(ssize_t)sizeof(e)){
   if(e.type!=EV_KEY)continue;
   if(e.code==KEY_F12&&e.value==1){overlay=!overlay;if(game)atomic_store(&game->keys,0);continue;}
   if(e.code==KEY_ENTER&&e.value==1&&pid<0){game_start();continue;}
   if(e.code==KEY_ESC&&e.value==1){if(overlay)overlay=0;else if(pid>0)game_stop();continue;}
   uint32_t b=0;
   if(e.code==KEY_LEFT||e.code==KEY_A)b=BTN_LEFT;
   else if(e.code==KEY_RIGHT||e.code==KEY_D)b=BTN_RIGHT;
   else if(e.code==KEY_UP||e.code==KEY_W)b=BTN_UP;
   else if(e.code==KEY_DOWN||e.code==KEY_S)b=BTN_DOWN;
   else if(e.code==KEY_SPACE)b=BTN_FIRE;
   if(b&&game){if(e.value&&!overlay)atomic_fetch_or(&game->keys,b);else atomic_fetch_and(&game->keys,~b);}
  }
 }
}
int main(int argc,char **argv){
 int frames=0,serve=0;const char *screenshot=0;
 const char *slash=strrchr(argv[0],'/');
 if(slash){size_t len=(size_t)(slash-argv[0]);if(len<sizeof(game_bin)-20){memcpy(game_bin,argv[0],len);game_bin[len]=0;strcat(game_bin,"/pocket-game");}}
 for(int i=1;i<argc;i++){
  if(!strcmp(argv[i],"--headless"))headless=1;
  else if(!strcmp(argv[i],"--serve"))serve=1;
  else if(!strcmp(argv[i],"--frames")&&i+1<argc)frames=atoi(argv[++i]);
  else if(!strcmp(argv[i],"--screenshot")&&i+1<argc)screenshot=argv[++i];
  else if(!strcmp(argv[i],"--game-bin")&&i+1<argc)snprintf(game_bin,sizeof(game_bin),"%s",argv[++i]);
  else if(!strcmp(argv[i],"--socket")&&i+1<argc)snprintf(sock_path,sizeof(sock_path),"%s",argv[++i]);
  else if(!strcmp(argv[i],"--help")){puts("pocket-session --headless --serve --frames N --screenshot FILE --game-bin PATH --socket PATH");return 0;}
  else return 2;
 }
 signal(SIGINT,on_signal);signal(SIGTERM,on_signal);signal(SIGPIPE,SIG_IGN);
 if(!headless&&init_fb()<0){fprintf(stderr,"/dev/fb0 missing or unusable; use --headless or enable fbdev emulation\n");return 1;}
 if((!headless||serve)&&init_socket()<0){perror("socket");return 1;}
 if(!headless)input_scan();
 int tick=0;
 while(!stop&&!sigstop){
  if(server>=0)command();input_poll();
  if(pid>0){int status;pid_t p=waitpid(pid,&status,WNOHANG);if(p==pid){pid=-1;game_stop();}}
  render();present();
  if(frames>0&&++tick>=frames)break;
  if(headless&&!serve&&frames==0)break;
  ms(33);
 }
 if(screenshot)shot(screenshot);
 game_stop();for(int i=0;i<input_count;i++)close(input[i]);
 if(server>=0){close(server);unlink(sock_path);}
 if(fbmem)munmap(fbmem,fbsize);if(fbfd>=0)close(fbfd);
 return 0;
}
