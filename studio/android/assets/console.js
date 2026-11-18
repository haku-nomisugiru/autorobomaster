"use strict";
const el=id=>document.getElementById(id);
const stages=["requirements","reuse","prototype","modeling","mates"];
const names=["确认组件需求","检索可复用组件","一次性原型机","建模与验收","添加机械配合"];
const drafts=["","","","",""];
let page=0, model=null, online=false, polling=false, sending=false, seq=0, timer, stamp="", pairing=false, stoppedId="";
const callbacks=new Map();
function rpc(path,body={}){return new Promise(resolve=>{callbacks.set(++seq,resolve);Console.call(seq,path,JSON.stringify(body));});}
window.reply=(id,raw)=>{const fn=callbacks.get(id);callbacks.delete(id);if(fn)fn(JSON.parse(raw));};
window.notice=text=>{el("toast").textContent=text;clearTimeout(timer);timer=setTimeout(()=>el("toast").textContent="",4300);};
function showPair(open){
  pairing=open;el("pair").hidden=!open;el("work").hidden=open;
  el("pair").querySelector(".cover p").textContent=Console.device()?"当前连接："+Console.device():"打开电脑 Studio 的「设置 → 手机控制台」，扫描配对二维码。";
  el("forget").hidden=!Console.device();el("back").hidden=!Console.device();
  el("scan").disabled=!!Console.device();el("album").disabled=!!Console.device();
}
window.scanResult=data=>{if(Console.device())return notice("请先解除原配对");el("scan").disabled=true;Console.pair(data);};
window.paired=error=>{showPair(!Console.device());notice(error||"请在电脑上批准这台设备");void sync();};
el("scan").onclick=()=>Console.scan();el("album").onclick=()=>Console.album();
el("back").onclick=()=>showPair(false);el("device").onclick=()=>showPair(true);
el("forget").onclick=()=>{if(confirm("解除这台手机的配对并清除本机待回执记录？")){Console.forget();model=null;online=false;stamp="";showPair(true);}};
const labels=["需求","检索","原型","验收","配合"];
labels.forEach((label,i)=>{const b=document.createElement("button");b.textContent=label;b.onclick=()=>flip(i);el("nav").append(b);});
function flip(i){
  if(i<0||i>=5)return;
  drafts[page]=el("input").value;page=i;el("input").value=drafts[page];stamp="";render();el("page").scrollTop=0;
}
let start=null;
el("page").addEventListener("touchstart",e=>{start=e.touches.length===1?[e.touches[0].clientX,e.touches[0].clientY]:null;},{passive:true});
el("page").addEventListener("touchend",e=>{
  if(!start)return;const dx=e.changedTouches[0].clientX-start[0],dy=e.changedTouches[0].clientY-start[1];start=null;
  if(Math.abs(dx)>50&&Math.abs(dx)>Math.abs(dy)*1.4)flip(page+(dx<0?1:-1));
},{passive:true});
function add(root,tag,text,cls=""){const n=document.createElement(tag);n.textContent=text;n.className=cls;root.append(n);return n;}
function render(){
  const m=model,can=online&&m?.active&&m.mode==="control"&&!m.busy&&!Console.pending();
  el("status").textContent=!online?"已断线 · 正在重连":m?.pendingApproval?"等待电脑批准":!m?.active?"电脑已切换项目":Console.pending()?"正在核对提交回执":m.busy?(stoppedId===m.operation?.id?"已请求停止":"任务执行中"):m.pending?"等待选择":m.mode==="read"?"只读连接":"已连接";
  el("project").textContent=m?.name||"手机控制台";
  el("stage").textContent=names[page];Array.from(el("nav").children).forEach((b,i)=>b.setAttribute("aria-current",String(i===page)));
  el("send").disabled=!can||!!m?.pending;el("input").disabled=!can||!!m?.pending;el("input").hidden=page===4;
  el("send").textContent=page===4?"开始添加配合":"发送";
  el("stop").hidden=!(m?.busy&&m?.operation?.status==="running"&&m.mode==="control"&&m.active);
  el("stop").disabled=!online||stoppedId===m?.operation?.id;
  const signature=JSON.stringify([page,m,can]);if(signature===stamp)return;stamp=signature;
  const choices=el("decision");choices.replaceChildren();
  if(m?.pending){const q=m.pending,box=add(choices,"section","");add(box,"strong",q.question);
    if(q.context)add(box,"p",q.context,"muted");
    q.options.forEach(c=>{const b=add(box,"button",c);b.disabled=!can;b.onclick=()=>submit(null,c,q);});
  }
  const history=el("history");history.replaceChildren();
  (m?.previews||[]).filter(p=>p.stage===stages[page]).forEach(p=>{
    const box=add(history,"article",""),b=add(box,"button",p.iteration?"查看第 "+p.iteration+" 轮模型视图":"查看模型视图");
    b.onclick=async()=>{b.disabled=true;const r=await rpc("preview",{id:p.id});if(r.data?.image){const img=document.createElement("img");img.className="thumb";img.alt="SolidWorks 模型视图";img.src=r.data.image;box.replaceChildren(img);}else{b.disabled=false;notice("未能读取模型视图");}};
  });
  const messages=(m?.messages||[]).filter(x=>x.stage===stages[page]);
  messages.forEach(x=>{const a=add(history,"article","",x.role);add(a,"small",({user:"你",assistant:"Studio",error:"执行出错",tool:"工具"})[x.role]||"工作台");add(a,"p",x.content);});
  if(!messages.length)add(history,"p","本阶段暂无消息","muted");
  const info=el("spec");info.replaceChildren();
  for(const [key,title] of Object.entries({requirements:"明确要求",verifiedParts:"已核实零件",assumptions:"暂定参数",relations:"参数关系",acceptance:"验收条件"})){
    const list=m?.spec?.[key]||[];if(list.length){add(info,"b",title);list.forEach(i=>add(info,"p",i.text+(i.source?" · "+i.source:"")));}
  }
  (m?.iterations||[]).forEach(i=>add(info,"p","第 "+i.number+" 轮 · "+i.title+" · "+i.status+"\n"+i.evidence));
  if(!info.children.length)add(info,"p","暂无规格与迭代记录","muted");
  if(m?.operation?.detail)add(info,"p",m.operation.detail);
}
async function flush(){
  if(sending)return;
  const raw=Console.pending();if(!raw)return;
  sending=true;try{
    const op=JSON.parse(raw);const result=await rpc("operation",op);
    if(Console.pending()!==raw)return;
    if(result.status===200){
      if(result.data?.status==="running"&&model)model.busy=true;
      Console.savePending("");const index=stages.indexOf(op.stage);
      if(op.text&&drafts[index]===op.text)drafts[index]="";
      if(page===index&&el("input").value.trim()===op.text)el("input").value="";
      notice("电脑已接收");
    }else if(result.status===409||result.status===403||result.status===422){Console.savePending("");notice(result.error||"请求已失效");}
    else {online=false;}
  }finally{sending=false;render();}
}
async function submit(text,choice,q){
  if(Console.pending()||!model||!online||model.busy)return;
  const op={id:Console.uuid(),project:model.project,stage:q?q.stage:stages[page],text,choice,questionId:q?q.callId:null};
  if(!Console.savePending(JSON.stringify(op)))return notice("提交记录保存失败");
  render();await flush();void sync();
}
el("send").onclick=()=>{const t=page===4?"开始添加机械配合，请按本项目的配合提示词执行。":el("input").value.trim();if(t)void submit(t,null,null);};
el("stop").onclick=async()=>{const id=model?.operation?.id;if(id){const r=await rpc("stop",{id});if(r.data?.requested)stoppedId=id;notice(r.data?.requested?"已请求停止":"请刷新任务状态");render();}};
async function sync(){
  if(polling||!Console.device()||document.hidden)return;
  polling=true;const host=Console.device();try{
    const r=await rpc("state");if(host!==Console.device())return;
    online=r.status===200;
    if(online){model=r.data;if(!model.pendingApproval&&Console.pending())await flush();}
    if(r.status===403){model=null;el("status").textContent="设备未授权，打开设备设置重新配对";}
    render();if(r.status===403)el("status").textContent="设备未授权 · 请重新配对";
  }finally{polling=false;}
}
showPair(!Console.device());render();void sync();setInterval(sync,1800);
document.addEventListener("visibilitychange",()=>{if(!document.hidden)void sync();});

