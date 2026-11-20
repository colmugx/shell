/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _GNU_SOURCE
#include "../include/pocket.h"
#include <sys/mman.h>
#include <stdlib.h>
#include <stdio.h>
#include <time.h>
#include <unistd.h>
static void ms(int n){struct timespec t={n/1000,(long)(n%1000)*1000000L};nanosleep(&t,0);}
int main(void){
 const char *s=getenv("POCKET_GAME_FD");if(!s)return 2;
 int fd=atoi(s);PocketFrame *p=mmap(0,sizeof(*p),PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);
 if(p==MAP_FAILED){perror("game mmap");return 3;}
 int x=POCKET_W/2,y=POCKET_H/2,tick=0;
 while(atomic_load(&p->alive)){
  uint32_t k=atomic_load(&p->keys);
  if(k&BTN_LEFT)x-=3;if(k&BTN_RIGHT)x+=3;
  if(k&BTN_UP)y-=3;if(k&BTN_DOWN)y+=3;
  if(x<8)x=8;if(x>POCKET_W-9)x=POCKET_W-9;
  if(y<8)y=8;if(y>POCKET_H-9)y=POCKET_H-9;
  for(int yy=0;yy<POCKET_H;yy++)for(int xx=0;xx<POCKET_W;xx++){
   uint32_t c=0x111827;
   if((xx+yy+tick/4)%53==0)c=0x30445e;
   int dx=xx-x,dy=yy-y;
   if(dx*dx+dy*dy<80)c=(k&BTN_FIRE)?0xffbc6e:0x6ce6d5;
   p->pixels[(size_t)yy*POCKET_W+xx]=c;
  }
  atomic_fetch_add(&p->frame,1);tick++;ms(33);
 }
 munmap(p,sizeof(*p));return 0;
}
