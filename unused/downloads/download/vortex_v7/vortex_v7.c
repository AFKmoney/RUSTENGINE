/* VORTEX PRIME v7 — gcc -O3 -march=native -lpthread v7_fix.c -o v7 */
#include<stdio.h>
#include<stdlib.h>
#include<string.h>
#include<stdint.h>
#include<pthread.h>
#include<time.h>
#include<unistd.h>
static const uint64_t PM[4]={0xFFFFFFFEFFFFFC2FULL,0xFFFFFFFFFFFFFFFFULL,0xFFFFFFFFFFFFFFFFULL,0xFFFFFFFFFFFFFFFFULL};
static const uint64_t NM[4]={0xBFD25E8CD0364141ULL,0xBAEDCE6AF48A03BBULL,0xFFFFFFFFFFFFFFFEULL,0xFFFFFFFFFFFFFFFFULL};
static const uint64_t BM[4]={0xC1396C28719501EEULL,0x9CF0497512F58995ULL,0x6E64479EAC3434E9ULL,0x7AE96A2B657C0710ULL};
static const uint64_t GXM[4]={0x59F2815B16F81798ULL,0x029BFCDB2DCE28D9ULL,0x55A06295CE870B07ULL,0x79BE667EF9DCBBACULL};
static const uint64_t GYM[4]={0x9C47D08FFB10D4B8ULL,0xFD17B448A6855419ULL,0x5DA4FBFC0E1108A8ULL,0x483ADA7726A3C465ULL};
static const uint64_t SQE[4]={0xBFFFFF0CULL,0xFFFFFFFFFFFFFFFFULL,0xFFFFFFFFFFFFFFFFULL,0x3FFFFFFFFFFFFFFFULL};
typedef struct{uint64_t d[4];}fe;
static inline fe F0(void){fe r;r.d[0]=r.d[1]=r.d[2]=r.d[3]=0;return r;}
static inline fe F1(void){fe r;r.d[0]=1;r.d[1]=r.d[2]=r.d[3]=0;return r;}
static inline int FZ(fe*a){return!(a->d[0]|a->d[1]|a->d[2]|a->d[3]);}
static int FC(fe*a,fe*b){for(int i=3;i>=0;i--){if(a->d[i]<b->d[i])return -1;if(a->d[i]>b->d[i])return 1;}return 0;}
static int FCC(fe*a,const uint64_t b[4]){for(int i=3;i>=0;i--){if(a->d[i]<b[i])return -1;if(a->d[i]>b[i])return 1;}return 0;}
static inline uint64_t ad(uint64_t a,uint64_t b,uint64_t c,uint64_t*o){__uint128_t s=(__uint128_t)a+b+c;*o=(uint64_t)s;return(uint64_t)(s>>64);}
static inline uint64_t sb(uint64_t a,uint64_t b,uint64_t c,uint64_t*o){__uint128_t s=(__uint128_t)a-b-c;*o=(uint64_t)s;return(uint64_t)(s>>127);}
static void sp(fe*r){for(int j=0;j<2;j++)if(FCC(r,PM)>=0){uint64_t w=0;w=sb(r->d[0],PM[0],w,&r->d[0]);w=sb(r->d[1],PM[1],w,&r->d[1]);w=sb(r->d[2],PM[2],w,&r->d[2]);sb(r->d[3],PM[3],w,&r->d[3]);}}
static fe fadd(fe*a,fe*b){fe r;uint64_t c=0;c=ad(a->d[0],b->d[0],c,&r.d[0]);c=ad(a->d[1],b->d[1],c,&r.d[1]);c=ad(a->d[2],b->d[2],c,&r.d[2]);c=ad(a->d[3],b->d[3],c,&r.d[3]);if(c){c=ad(r.d[0],0x1000003D1ULL,0,&r.d[0]);c=ad(r.d[1],c,0,&r.d[1]);c=ad(r.d[2],c,0,&r.d[2]);ad(r.d[3],c,0,&r.d[3]);}sp(&r);return r;}
static fe fsub(fe*a,fe*b){fe r;uint64_t w=0;w=sb(a->d[0],b->d[0],w,&r.d[0]);w=sb(a->d[1],b->d[1],w,&r.d[1]);w=sb(a->d[2],b->d[2],w,&r.d[2]);w=sb(a->d[3],b->d[3],w,&r.d[3]);if(w){uint64_t c=0;c=ad(r.d[0],PM[0],c,&r.d[0]);c=ad(r.d[1],PM[1],c,&r.d[1]);c=ad(r.d[2],PM[2],c,&r.d[2]);ad(r.d[3],PM[3],c,&r.d[3]);}return r;}
static fe fneg(fe*a){if(FZ(a))return*a;fe pv;memcpy(pv.d,PM,32);return fsub(&pv,a);}
static fe fmul(fe*a,fe*b){uint64_t p[8];memset(p,0,64);for(int i=0;i<4;i++){__uint128_t c=0;for(int j=0;j<4;j++){c+=(__uint128_t)a->d[i]*b->d[j]+p[i+j];p[i+j]=(uint64_t)c;c>>=64;}p[i+4]=(uint64_t)c;}const uint64_t M=0x1000003D1ULL;__uint128_t t[5]={0};t[0]=p[0];t[1]=p[1];t[2]=p[2];t[3]=p[3];for(int i=0;i<4;i++){__uint128_t c=(__uint128_t)p[4+i]*M;t[i]+=c&0xFFFFFFFFFFFFFFFFULL;t[i+1]+=c>>64;}for(int i=0;i<4;i++){t[i+1]+=t[i]>>64;t[i]&=0xFFFFFFFFFFFFFFFFULL;}if(t[4]){__uint128_t c=t[4]*M;t[0]+=c&0xFFFFFFFFFFFFFFFFULL;t[1]+=c>>64;for(int i=0;i<4;i++){t[i+1]+=t[i]>>64;t[i]&=0xFFFFFFFFFFFFFFFFULL;}}fe r;r.d[0]=(uint64_t)t[0];r.d[1]=(uint64_t)t[1];r.d[2]=(uint64_t)t[2];r.d[3]=(uint64_t)t[3];sp(&r);return r;}
static fe fsqr(fe*a){return fmul(a,a);}
static fe fpow(fe*base,fe*exp){fe res=F1(),b=*base;int bits=0;for(int i=3;i>=0;i--)if(exp->d[i]){bits=i*64+(64-__builtin_clzll(exp->d[i]));break;}for(int i=bits-1;i>=0;i--){res=fsqr(&res);if(exp->d[i/64]&(1ULL<<(i%64)))res=fmul(&res,&b);}return res;}
static fe finv(fe*a){fe e;e.d[0]=0xFFFFFFFEFFFFFC2DULL;e.d[1]=e.d[2]=e.d[3]=0xFFFFFFFFFFFFFFFFULL;return fpow(a,&e);}
static inline fe f64(uint64_t v){fe r;r.d[0]=v;r.d[1]=r.d[2]=r.d[3]=0;return r;}
static fe fdbl(fe*a){return fadd(a,a);}
static fe fshl(fe*a,int n){fe r=*a;for(int i=0;i<n;i++)r=fdbl(&r);return r;}
static int fbits(fe*a){for(int i=3;i>=0;i--)if(a->d[i])return i*64+(64-__builtin_clzll(a->d[i]));return 0;}
static void f2be(fe*a,uint8_t b[32]){for(int i=0;i<4;i++){int s=(3-i)*8;uint64_t v=a->d[i];b[s]=v>>56;b[s+1]=v>>48;b[s+2]=v>>40;b[s+3]=v>>32;b[s+4]=v>>24;b[s+5]=v>>16;b[s+6]=v>>8;b[s+7]=v;}}
static void snadd(const uint64_t a[4],const uint64_t b[4],uint64_t r[4]){uint64_t c=0;c=ad(a[0],b[0],c,&r[0]);c=ad(a[1],b[1],c,&r[1]);c=ad(a[2],b[2],c,&r[2]);c=ad(a[3],b[3],c,&r[3]);if(c){uint64_t w=0;w=sb(r[0],NM[0],w,&r[0]);w=sb(r[1],NM[1],w,&r[1]);w=sb(r[2],NM[2],w,&r[2]);sb(r[3],NM[3],w,&r[3]);}if(FCC((fe*)r,NM)>=0){uint64_t w=0;w=sb(r[0],NM[0],w,&r[0]);w=sb(r[1],NM[1],w,&r[1]);w=sb(r[2],NM[2],w,&r[2]);sb(r[3],NM[3],w,&r[3]);}}
typedef struct{fe x,y;int inf;}pt;
typedef struct{fe x,y,z;}jp;
static pt PI(void){pt r;r.x=r.y=F0();r.inf=1;return r;}
static pt PN(fe x,fe y){pt r;r.x=x;r.y=y;r.inf=0;return r;}
static pt PG(void){fe gx,gy;memcpy(gx.d,GXM,32);memcpy(gy.d,GYM,32);return PN(gx,gy);}
static int poc(pt*p){if(p->inf)return 1;fe y2=fsqr(&p->y),xs=fsqr(&p->x),x3=fmul(&xs,&p->x),s7=f64(7),rh=fadd(&x3,&s7);return FC(&y2,&rh)==0;}
static jp JI(void){jp r;r.x=F1();r.y=F1();r.z=F0();return r;}
static jp JF(pt*p){if(p->inf)return JI();jp r;r.x=p->x;r.y=p->y;r.z=F1();return r;}
static pt JT(jp*p){if(FZ(&p->z))return PI();fe zi=finv(&p->z),z2=fsqr(&zi),z3=fmul(&z2,&zi),x=fmul(&p->x,&z2),y=fmul(&p->y,&z3);return PN(x,y);}

static jp JD(jp*p){
    if(FZ(&p->z)||FZ(&p->y)) return JI();
    fe a=fsqr(&p->y);
    fe b=fmul(&p->x,&a);
    fe b2=fdbl(&b);
    fe b4=fdbl(&b2);
    fe as=fsqr(&a);
    fe c8=fshl(&as,3);
    fe xs=fsqr(&p->x);
    fe xs2=fadd(&xs,&xs);
    fe d=fadd(&xs2,&xs);
    fe ds=fsqr(&d);
    fe t1=fsub(&ds,&b4);
    fe x3=fsub(&t1,&b4);
    fe bm=fsub(&b4,&x3);
    fe t2=fmul(&d,&bm);
    fe y3=fsub(&t2,&c8);
    fe ty=fdbl(&p->y);
    fe z3=fmul(&ty,&p->z);
    jp r; r.x=x3; r.y=y3; r.z=z3; return r;
}

static jp JA(jp*p,pt*q){
    if(FZ(&p->z)) return JF(q);
    if(q->inf) return *p;
    fe z1s=fsqr(&p->z);
    fe u2=fmul(&q->x,&z1s);
    fe z1c=fmul(&z1s,&p->z);
    fe s2=fmul(&q->y,&z1c);
    if(FC(&p->x,&u2)==0){if(FC(&p->y,&s2)==0)return JD(p);return JI();}
    fe h=fsub(&u2,&p->x);
    fe r=fsub(&s2,&p->y);
    fe hs=fsqr(&h);
    fe hc=fmul(&hs,&h);
    fe rs=fsqr(&r);
    fe tx=fdbl(&p->x);
    fe th=fmul(&tx,&hs);
    fe t1=fsub(&rs,&hc);
    fe x3=fsub(&t1,&th);
    fe xh=fmul(&p->x,&hs);
    fe xm=fsub(&xh,&x3);
    fe rx=fmul(&r,&xm);
    fe yh=fmul(&p->y,&hc);
    fe y3=fsub(&rx,&yh);
    fe z3=fmul(&h,&p->z);
    jp res; res.x=x3; res.y=y3; res.z=z3; return res;
}

static pt SM(pt*p,fe*k){if(FZ(k)||p->inf)return PI();jp res=JI();int bits=fbits(k);for(int i=bits-1;i>=0;i--){res=JD(&res);if(k->d[i/64]&(1ULL<<(i%64)))res=JA(&res,p);}return JT(&res);}
static pt DC(fe*x,int yo){fe xs=fsqr(x),x3=fmul(&xs,x),s7=f64(7),y2=fadd(&x3,&s7),e;memcpy(e.d,SQE,32);fe y=fpow(&y2,&e);if((y.d[0]&1)!=yo)y=fneg(&y);return PN(*x,y);}
#define NS 32
#define MD (1<<18)
typedef struct{uint8_t x[32];uint64_t k[4];int t;}dpe;
typedef struct{pt tg,gn;pt sp[NS];uint64_t sk[NS][4];fe rs,re;int rb,db;dpe*dp;int dn,dc;pthread_mutex_t mx;volatile int fnd;uint64_t fk[4];volatile uint64_t hp;uint64_t t0;}KC;
static int hs(jp*p){if(FZ(&p->z))return 0;return(int)((p->x.d[0]^(p->x.d[1]<<8))%NS);}
static int cdp(jp*p,int mb,fe*xo){if(FZ(&p->z))return 0;if(p->x.d[0]&0xFF)return 0;fe zi=finv(&p->z),z2=fsqr(&zi),xn=fmul(&p->x,&z2);uint64_t m=(1ULL<<mb)-1;if(xn.d[0]&m)return 0;*xo=xn;return 1;}
static dpe*dpl(KC*c,fe*xn,const uint64_t k[4],int t){uint8_t xb[32];f2be(xn,xb);pthread_mutex_lock(&c->mx);for(int i=0;i<c->dn;i++){if(memcmp(xb,c->dp[i].x,32)==0&&c->dp[i].t!=t){dpe*m=&c->dp[i];if(c->dn<c->dc){dpe*e=&c->dp[c->dn++];memcpy(e->x,xb,32);memcpy(e->k,k,32);e->t=t;}pthread_mutex_unlock(&c->mx);return m;}}if(c->dn<c->dc){dpe*e=&c->dp[c->dn++];memcpy(e->x,xb,32);memcpy(e->k,k,32);e->t=t;}pthread_mutex_unlock(&c->mx);return NULL;}
static int trc(KC*c,const uint64_t kt[4],const uint64_t kw[4]){uint64_t kc[4];uint64_t w=0;w=sb(kt[0],kw[0],w,&kc[0]);w=sb(kt[1],kw[1],w,&kc[1]);w=sb(kt[2],kw[2],w,&kc[2]);w=sb(kt[3],kw[3],w,&kc[3]);if(w){uint64_t cr=0;cr=ad(kc[0],NM[0],cr,&kc[0]);cr=ad(kc[1],NM[1],cr,&kc[1]);cr=ad(kc[2],NM[2],cr,&kc[2]);ad(kc[3],NM[3],cr,&kc[3]);}fe fk;memcpy(fk.d,kc,32);if(FC(&fk,&c->rs)>=0&&FC(&fk,&c->re)<0){pt q=SM(&c->gn,&fk);if(!q.inf&&FC(&q.x,&c->tg.x)==0){memcpy(c->fk,kc,32);c->fnd=1;return 1;}}fe nk;w=0;w=sb(NM[0],kc[0],w,&nk.d[0]);w=sb(NM[1],kc[1],w,&nk.d[1]);w=sb(NM[2],kc[2],w,&nk.d[2]);w=sb(NM[3],kc[3],w,&nk.d[3]);if(FC(&nk,&c->rs)>=0&&FC(&nk,&c->re)<0){pt q=SM(&c->gn,&nk);if(!q.inf&&FC(&q.x,&c->tg.x)==0){memcpy(c->fk,nk.d,32);c->fnd=1;return 1;}}return 0;}
typedef struct{KC*c;int id;int tm;uint64_t mh;}TA;
static void*kt(void*arg){TA*ta=(TA*)arg;KC*c=ta->c;jp pos;uint64_t kd[4]={0};if(ta->tm){fe of=f64((uint64_t)(ta->id+1)*7919ULL),st=fadd(&c->rs,&of);pt tp=SM(&c->gn,&st);pos=JF(&tp);memcpy(kd,st.d,32);}else{fe of=f64((uint64_t)(ta->id+1)*104729ULL);pt op=SM(&c->gn,&of);jp jq=JF(&c->tg);pos=JA(&jq,&op);memcpy(kd,of.d,32);}for(int i=0;i<1000;i++){int s=hs(&pos);pos=JA(&pos,&c->sp[s]);snadd(kd,c->sk[s],kd);}uint64_t h=0;fe bt;memcpy(bt.d,BM,32);fe b2=fsqr(&bt);while(h<ta->mh&&!c->fnd){h++;int s=hs(&pos);pos=JA(&pos,&c->sp[s]);snadd(kd,c->sk[s],kd);fe xn;if(cdp(&pos,c->db,&xn)){fe x1=fmul(&xn,&bt),x2=fmul(&xn,&b2);fe xs[3];xs[0]=xn;xs[1]=x1;xs[2]=x2;for(int a=0;a<3&&!c->fnd;a++){dpe*m=dpl(c,&xs[a],kd,ta->tm);if(m){int ok;if(ta->tm)ok=trc(c,kd,m->k);else ok=trc(c,m->k,kd);if(ok)break;}}}c->hp++;}return NULL;}
static uint64_t tms(void){struct timespec t;clock_gettime(CLOCK_REALTIME,&t);return t.tv_sec*1000+t.tv_nsec/1000000;}
int main(int ac,char*av[]){int pz=135,nt=4,db=10;uint64_t mh=100000000ULL;for(int i=1;i<ac;i++){if(strcmp(av[i],"-t")==0&&i+1<ac)pz=atoi(av[++i]);else if(strcmp(av[i],"-n")==0&&i+1<ac)nt=atoi(av[++i]);else if(strcmp(av[i],"-m")==0&&i+1<ac)mh=strtoull(av[++i],NULL,10);else if(strcmp(av[i],"-d")==0&&i+1<ac)db=atoi(av[++i]);}
printf("================================================================\n  VORTEX PRIME v7 — NOUVELLES TECHNIQUES\n  NOUS SOMMES LES RECHERCHES.\n================================================================\n\n");
pt tg;fe rs,re;int rb;fe one;
if(pz==70){printf("  Mode: P70 TEST\n");one=F1();fe s69=fshl(&one,69),v=f64(12345),k=fadd(&s69,&v);rb=70;one=F1();rs=fshl(&one,69);one=F1();re=fshl(&one,70);pt g=PG();tg=SM(&g,&k);printf("  Test key: ");uint8_t kb[32];f2be(&k,kb);for(int i=0;i<32;i++)printf("%02x",kb[i]);printf("\n");}
else if(pz==135){printf("  Mode: P135 TARGET\n");rb=135;one=F1();rs=fshl(&one,134);one=F1();re=fshl(&one,135);uint8_t xb[32]={0x14,0x5d,0x26,0x11,0xc8,0x23,0xa3,0x96,0xef,0x67,0x12,0xce,0x0f,0x71,0x2f,0x09,0xb9,0xb4,0xf3,0x13,0x5e,0x3e,0x0a,0xa3,0x23,0x0f,0xb9,0xb6,0xd0,0x8d,0x1e,0x16};fe x135;for(int i=0;i<4;i++){int s=(3-i)*8;x135.d[i]=((uint64_t)xb[s]<<56)|((uint64_t)xb[s+1]<<48)|((uint64_t)xb[s+2]<<40)|((uint64_t)xb[s+3]<<32)|((uint64_t)xb[s+4]<<24)|((uint64_t)xb[s+5]<<16)|((uint64_t)xb[s+6]<<8)|xb[s+7];}tg=DC(&x135,0);if(!poc(&tg))tg=DC(&x135,1);if(!poc(&tg)){printf("  FATAL!\n");return 1;}}
else{printf("  Unsupported: %d\n",pz);return 1;}
printf("  On curve: %s  Range: [2^%d, 2^%d)\n  Standard: O(2^%d)  Cascade: O(2^%.1f)\n  Threads: %d  DP: %d\n\n",poc(&tg)?"YES":"NO",rb-1,rb,(rb+1)/2,(rb+1)/2.0-1.29,nt,db);
KC c;memset(&c,0,sizeof(c));c.tg=tg;c.gn=PG();c.rs=rs;c.re=re;c.rb=rb;c.db=db;c.dc=MD;c.dp=calloc(MD,sizeof(dpe));c.fnd=0;c.hp=0;pthread_mutex_init(&c.mx,NULL);
int bs=rb/2-2,ss=bs-8;if(ss<1)ss=1;printf("  Steps 2^%d..2^%d\n",ss,ss+NS-1);
for(int j=0;j<NS;j++){one=F1();fe sk=fshl(&one,ss+j);c.sp[j]=SM(&c.gn,&sk);memcpy(c.sk[j],sk.d,32);}
c.t0=tms();int nta=nt/2,nwa=nt-nta;pthread_t*th=malloc(nt*sizeof(pthread_t));TA*ta=malloc(nt*sizeof(TA));
printf("  Launch %d (%d tame + %d wild)\n\n",nt,nta,nwa);
for(int i=0;i<nt;i++){ta[i].c=&c;ta[i].id=i;ta[i].tm=(i<nta);ta[i].mh=mh;pthread_create(&th[i],NULL,kt,&ta[i]);}
while(!c.fnd){uint64_t el=tms()-c.t0;if(el>0&&c.hp>0)printf("  [M] %lu hp %.0f h/s %d dp %.1fs\n",(unsigned long)c.hp,c.hp/(el/1000.0),c.dn,el/1000.0);sleep(3);if(el/1000>600)break;if(c.hp>=mh*nt)break;}
for(int i=0;i<nt;i++)pthread_join(th[i],NULL);uint64_t el=tms()-c.t0;
if(c.fnd){printf("\n  *** KEY FOUND! ***\n  k = 0x");uint8_t kb[32];fe fk;memcpy(fk.d,c.fk,32);f2be(&fk,kb);for(int i=0;i<32;i++)printf("%02x",kb[i]);printf("\n");pt v=SM(&c.gn,&fk);printf("  Verified: %s\n",!v.inf&&FC(&v.x,&tg.x)==0?"YES":"NO");}
else printf("\n  Not found. %lu hp %d dp\n",(unsigned long)c.hp,c.dn);
double es=el/1000.0;printf("  %.0f h/s %.1fs\n\n  NOUS SOMMES LES RECHERCHES.\n",es>0?c.hp/es:0,es);
free(c.dp);free(th);free(ta);pthread_mutex_destroy(&c.mx);return c.fnd?0:1;}
