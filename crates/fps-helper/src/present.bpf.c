// SPDX-License-Identifier: GPL-2.0
// x86_64 SysV ABI. No injection or writes to the traced process.
#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>
typedef unsigned long long u64;
typedef unsigned int u32;
struct regs { u64 r15,r14,r13,r12,bp,bx,r11,r10,r9,r8,ax,cx,dx,si,di,orig_ax,ip,cs,flags,sp,ss; };
struct present { u32 type; u32 pad0; u64 next; u32 waits; u32 pad1; u64 semaphores; u32 count; u32 pad2; u64 chains,indices,results; };
struct pending { u64 time; u64 results; u64 chains[8]; u32 count; u32 api; };
struct event { u64 time; u64 chain; u32 api; u32 pad; };
struct { __uint(type,BPF_MAP_TYPE_HASH); __uint(max_entries,4096); __type(key,u64); __type(value,struct pending); } calls SEC(".maps");
struct { __uint(type,BPF_MAP_TYPE_RINGBUF); __uint(max_entries,1048576); } events SEC(".maps");
struct { __uint(type,BPF_MAP_TYPE_ARRAY); __uint(max_entries,1); __type(key,u32); __type(value,u64); } dropped SEC(".maps");
static __always_inline void lose(void) {u32 key=0;u64 *n=bpf_map_lookup_elem(&dropped,&key);if(n)__sync_fetch_and_add(n,1);}
SEC("uprobe") int enter(struct regs *ctx) {
    u64 tid=bpf_get_current_pid_tgid();
    struct pending p={.time=bpf_ktime_get_ns(),.api=bpf_get_attach_cookie(ctx)};
    if (p.api==1) {
        struct present info={};
        if(bpf_probe_read_user(&info,sizeof(info),(void *)ctx->si) || !info.count || info.count>8) {lose();return 0;}
        p.count=info.count;p.results=info.results;
        for(int i=0;i<8;i++) {if(i>=p.count)break;if(bpf_probe_read_user(&p.chains[i],8,(void *)(info.chains+i*8))) {lose();return 0;}}
    } else {p.count=1;p.chains[0]=ctx->si;}
    if(bpf_map_lookup_elem(&calls,&tid)) {lose();return 0;}
    if(bpf_map_update_elem(&calls,&tid,&p,BPF_NOEXIST))lose();
    return 0;
}
SEC("uretprobe") int leave(struct regs *ctx) {
    u64 tid=bpf_get_current_pid_tgid();struct pending *p=bpf_map_lookup_elem(&calls,&tid);if(!p)return 0;
    for(int i=0;i<8;i++) {
        if(i>=p->count)break;
        int result=(int)ctx->ax;
        if(p->api==1 && p->results && bpf_probe_read_user(&result,4,(void *)(p->results+i*4))) {lose();continue;}
        if((p->api==1 && result!=0 && result!=1000001003) || (p->api==2 && !result))continue;
        struct event *e=bpf_ringbuf_reserve(&events,sizeof(*e),0);
        if(!e) {lose();continue;}
        e->time=p->time;e->chain=p->chains[i];e->api=p->api;e->pad=0;bpf_ringbuf_submit(e,0);
    }
    bpf_map_delete_elem(&calls,&tid);return 0;
}
char LICENSE[] SEC("license")="GPL";
