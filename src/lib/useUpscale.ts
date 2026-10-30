import { useCallback, useEffect, useRef, useState } from 'react';
import { callNative, errorMessage, isNative } from './native';
import { idleUpscale, upscaleActive, type UpscaleController, type UpscaleModel, type UpscaleRequest, type UpscaleStatus } from './upscale';
import type { Source } from './types';
export function useUpscale(enabled:boolean,callbacks:{onApply:(source:Source)=>Promise<boolean>;onChanged:()=>unknown}):UpscaleController {
 const [status,setStatus]=useState(idleUpscale);const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [models,setModels]=useState<UpscaleModel[]>([]);
 const alive=useRef(false);const locked=useRef(false);const latest=useRef(status);const generation=useRef(0);
 const finalizing=useRef(false);const [finishing,setFinishing]=useState(false);
 const autoApply=useRef<string|null>(null);const notified=useRef<string|null>(null);const dismissed=useRef<string|null>(null);
 const handlers=useRef(callbacks);handlers.current=callbacks;
 const loadModels=useCallback(async()=>{if(!isNative())return;try{const value=await callNative<UpscaleModel[]>('upscale_models');if(alive.current&&Array.isArray(value))setModels(value);}catch(e){if(alive.current)setError(errorMessage(e));}},[]);
 const receive=useCallback(async(next:UpscaleStatus|undefined)=>{
  if(!next||!alive.current)return;latest.current=next;
  if(next.jobId!==dismissed.current||upscaleActive(next))setStatus(next);
  if(!next.jobId||upscaleActive(next)||next.phase==='idle'||notified.current===next.jobId)return;
  notified.current=next.jobId;finalizing.current=true;setFinishing(true);
  try{
   await handlers.current.onChanged();
   void loadModels();
   if(autoApply.current===next.jobId){autoApply.current=null;if(next.phase==='ready'&&next.source){if(!await handlers.current.onApply(next.source)&&alive.current)setError('Đã lưu ảnh. Chưa áp dụng được.');}}
  }catch(e){if(alive.current)setError(errorMessage(e));}finally{finalizing.current=false;if(alive.current)setFinishing(false);}
 },[loadModels]);
 const active=upscaleActive(status);
 useEffect(()=>{
  alive.current=true;let disposed=false;let timer:ReturnType<typeof setTimeout>;
  const poll=async()=>{const current=generation.current;try{const next=await callNative<UpscaleStatus>('upscale_status');if(!disposed&&current===generation.current)await receive(next);}catch(e){if(!disposed&&alive.current)setError(errorMessage(e));}finally{if(!disposed)timer=setTimeout(poll,upscaleActive(latest.current)?750:5000);}};
  if(enabled&&isNative()){void poll();void loadModels();}
  return()=>{disposed=true;alive.current=false;clearTimeout(timer);};
 },[enabled,active,receive,loadModels]);
 const start=useCallback(async(request:UpscaleRequest)=>{
  if(locked.current||finalizing.current||upscaleActive(latest.current)||!isNative())return false;
  locked.current=true;setBusy(true);setError('');generation.current++;
  try{
   const args=request.all?{sourceId:request.sourceId??null,mode:request.mode,...(request.mode==='ai'?{model:request.model}: {})}:{ids:request.ids??[],mode:request.mode,...(request.mode==='ai'?{model:request.model}:{})};
   const next=await callNative<UpscaleStatus>(request.all?'upscale_start_all':'upscale_start',args);
   autoApply.current=request.apply?next.jobId:null;dismissed.current=null;await receive(next);return true;
  }catch(e){if(alive.current)setError(errorMessage(e));return false;}finally{locked.current=false;if(alive.current)setBusy(false);}
 },[receive]);
 const cancel=useCallback(async()=>{if(locked.current||finalizing.current||!latest.current.jobId)return;locked.current=true;setBusy(true);autoApply.current=null;try{await receive(await callNative<UpscaleStatus>('upscale_cancel',{jobId:latest.current.jobId}));}catch(e){if(alive.current)setError(errorMessage(e));}finally{locked.current=false;if(alive.current)setBusy(false);}},[receive]);
 const apply=useCallback(async()=>{const source=latest.current.source;if(locked.current||finalizing.current||upscaleActive(latest.current)||!source)return false;locked.current=true;setBusy(true);setError('');try{return await handlers.current.onApply(source);}catch(e){if(alive.current)setError(errorMessage(e));return false;}finally{locked.current=false;if(alive.current)setBusy(false);}},[]);
 const dismiss=useCallback(()=>{if(upscaleActive(latest.current))return;dismissed.current=latest.current.jobId;setStatus(idleUpscale);setError('');},[]);
 return {status,active:upscaleActive(status),busy:busy||finishing,error,models,start,loadModels,cancel,apply,dismiss};
}
