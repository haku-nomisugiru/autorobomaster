import { useEffect, useState } from "react";
import { call } from "./bridge";
type Device = { id:string; name:string; project:string; mode:string };
type Admin = { enabled:boolean; devices:Device[] };
export function MobilePanel({ project }: {project:string}) {
  const [address,setAddress]=useState(()=>localStorage.getItem("studio-mobile-address") || "");
  const [readOnly,setReadOnly]=useState(false);
  const [admin,setAdmin]=useState<Admin>({enabled:false,devices:[]});
  const [qr,setQr]=useState("");
  const [message,setMessage]=useState("");
  const refresh=async()=>setAdmin(await call<Admin>("mobile_admin"));
  useEffect(()=>{let alive=true;const tick=()=>void call<Admin>("mobile_admin").then(v=>{if(alive)setAdmin(v)}).catch(e=>{if(alive)setMessage(String(e))});tick();const timer=setInterval(tick,2000);return()=>{alive=false;clearInterval(timer)}},[]);
  useEffect(()=>{setQr("")},[project,address,readOnly]);
  useEffect(()=>{if(!qr)return;const timer=setTimeout(()=>setQr(""),120000);return()=>clearTimeout(timer)},[qr]);
  async function act(fn:()=>Promise<unknown>){try{setMessage("");await fn();await refresh()}catch(e){setMessage(String(e))}}
  return <><div className="settings-page-heading"><h2>手机控制台</h2></div><div className="settings-section">
    <div className="section-heading"><h3>连接安卓手机</h3><span>{admin.enabled?"已开启":"已关闭"}</span></div>
    <p>外网访问时，两端连接同一 Tailscale 网络。在电脑运行以下命令，把返回的 HTTPS 地址填在下方。</p>
    <code>tailscale serve --bg http://127.0.0.1:49731</code>
    <div className="form-grid"><label className="full-width">手机访问地址<input value={address} placeholder="https://电脑名.网络名.ts.net" onChange={e=>setAddress(e.target.value)}/></label>
    <label>本次配对权限<select value={readOnly?"read":"control"} onChange={e=>setReadOnly(e.target.value==="read")}><option value="control">操作当前项目</option><option value="read">仅查看</option></select></label></div>
    <div className="connection-actions"><button className="secondary" onClick={()=>void act(async()=>{const t=await call<{svg:string}>("mobile_ticket",{address,readOnly});setQr(t.svg);localStorage.setItem("studio-mobile-address",address)})}>生成配对码</button>
    {!admin.enabled&&<button className="secondary" onClick={()=>void act(()=>call("mobile_enable"))}>开启已配对连接</button>}
    {admin.enabled&&<button className="secondary" onClick={()=>void act(async()=>{await call("mobile_disable");setQr("")})}>关闭并撤销所有设备</button>}</div>
    {qr&&<figure><img width="238" height="238" alt="两分钟有效的一次性配对二维码" src={"data:image/svg+xml;charset=utf-8,"+encodeURIComponent(qr)}/><figcaption>用手机控制台扫码，然后在下方批准设备。两分钟内有效。</figcaption></figure>}
    {message&&<p role="alert">{message}</p>}</div>
    <div className="settings-section"><h3>配对设备</h3>{!admin.devices.length&&<p>暂无配对设备</p>}
    {admin.devices.map(d=><div className="connection-actions" key={d.id}><strong>{d.name}</strong><span>{d.mode.endsWith("read")?"仅查看":"可操作"} · {d.project===project?"当前项目":"其他项目"} · {d.mode.startsWith("pending")?"等待批准":"已配对"}</span>
    {d.mode.startsWith("pending")&&d.project===project&&<button className="secondary" onClick={()=>void act(()=>call("mobile_device",{device:d.id,approve:true}))}>批准</button>}
    <button className="secondary" onClick={()=>void act(()=>call("mobile_device",{device:d.id,approve:false}))}>撤销</button></div>)}</div></>
}

