/* SPDX-License-Identifier: GPL-3.0-or-later */
#include "../include/pocket.h"
#include <sys/socket.h>
#include <sys/un.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc,char **argv){
 if(argc<2){fprintf(stderr,"Usage: pocketctl STATUS | GAME START/STOP | OVERLAY OPEN/CLOSE | QUIT\n");return 2;}
 char msg[128]={0};for(int i=1;i<argc;i++){if(i>1)strcat(msg," ");strncat(msg,argv[i],sizeof(msg)-strlen(msg)-2);}strcat(msg,"\n");
 const char *path=getenv("POCKET_SOCKET");if(!path)path=POCKET_SOCK;
 int fd=socket(AF_UNIX,SOCK_STREAM,0);if(fd<0){perror("socket");return 1;}
 struct sockaddr_un a={.sun_family=AF_UNIX};if(strlen(path)>=sizeof(a.sun_path)){close(fd);return 2;}
 strcpy(a.sun_path,path);
 if(connect(fd,(void*)&a,sizeof(a))<0){perror("connect");close(fd);return 1;}
 (void)write(fd,msg,strlen(msg));ssize_t n=read(fd,msg,sizeof(msg)-1);
 if(n<=0){close(fd);return 1;}msg[n]=0;fputs(msg,stdout);close(fd);
 return strncmp(msg,"ERROR",5)==0?1:0;
}
