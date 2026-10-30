import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { imageUrl, isNative } from '../lib/native';
import type { Capabilities, MediaItem, Source } from '../lib/types';
import { formatBytes, type ModelId, type UpscaleController, type UpscaleMode } from '../lib/upscale';

interface Props {platform:Capabilities['platform'];sources:Source[];media:MediaItem[];initialIds?:string[];controller:UpscaleController;onClose:()=>void}
export function UpscaleDialog({platform,sources,media,initialIds=[],controller,onClose}:Props){
 const dialog=useRef<HTMLDialogElement>(null);const [mode,setMode]=useState<UpscaleMode>('fast');const [model,setModel]=useState<ModelId>('photo');
 const [sourceId,setSourceId]=useState('');const [selected,setSelected]=useState(initialIds.filter(id=>media.some(item=>item.id===id)).slice(0,50));
 const [apply,setApply]=useState(false);const [resultIndex,setResultIndex]=useState(0);const [showResults,setShowResults]=useState(controller.status.results.length>0);
 const {status,active,busy,error,models}=controller;const chosen=models.find(item=>item.id===model);const result=status.results[resultIndex]||status.results[0];
 useEffect(()=>{const element=dialog.current;element?.showModal();void controller.loadModels();return()=>element?.close();},[]);
 const start=async(all:boolean)=>{if(await controller.start({...(all?{all:true,sourceId:sourceId||null}:{ids:selected}),mode,...(mode==='ai'?{model}:{}),apply}))onClose();};
 const available=media.filter(item=>!sourceId||item.sourceId===sourceId);
 return <dialog ref={dialog} className="source-dialog upscale-dialog" aria-labelledby="upscale-title" onCancel={event=>{event.preventDefault();onClose();}}>
  <div className="dialog-heading"><h2 id="upscale-title">Nâng ảnh 4K</h2><button className="icon-button" aria-label="Đóng" onClick={onClose}><X size={20}/></button></div>
  {active?<p className="muted small">Đang chạy. Tiến trình ở cuối cửa sổ.</p>:showResults&&result?<>
   {status.results.length>1&&<label className="field">Ảnh kết quả<select value={resultIndex} onChange={event=>setResultIndex(Number(event.target.value))}>{status.results.map((item,index)=><option key={item.id} value={index}>{item.name}</option>)}</select></label>}
   <div className="upscale-comparison"><figure><img src={imageUrl(result.originalPreview)} alt="Trước"/><figcaption>Trước</figcaption></figure><figure><img src={imageUrl(result.resultPreview)} alt="Sau"/><figcaption>Sau · {result.width} × {result.height}</figcaption></figure></div>
   <p className="muted small">Đã lưu {status.completed} ảnh{status.failed?` · ${status.failed} ảnh lỗi`:''}{status.source?` · ${status.source.name}`:''}</p>
   <div className="dialog-footer"><button className="button secondary" onClick={()=>setShowResults(false)}>Chạy bộ khác</button>{status.source&&<button className="button primary" disabled={busy} onClick={()=>void controller.apply()}>Áp dụng</button>}</div>
  </>:<>
   <label className="field">Nguồn<select aria-label="Nguồn upscale" value={sourceId} disabled={busy} onChange={event=>setSourceId(event.target.value)}><option value="">Tất cả nguồn đang bật</option>{sources.filter(source=>source.enabled).map(source=><option key={source.id} value={source.id}>{source.name}</option>)}</select></label>
   <label className="field">Chế độ<select aria-label="Chế độ upscale" value={mode} disabled={busy} onChange={event=>setMode(event.target.value as UpscaleMode)}><option value="fast">Nhanh</option><option value="ai" disabled={platform!=='windows'}>AI</option></select></label>
   {mode==='ai'?<>
    <label className="field">Model<select aria-label="Model upscale" value={model} disabled={busy||!models.length} onChange={event=>setModel(event.target.value as ModelId)}>{models.map(item=><option key={item.id} value={item.id}>{item.name} · {formatBytes(item.bytes)} · {item.installed?'Đã tải':'Chưa tải'}</option>)}</select></label>
    {chosen&&<div className="model-details"><span>{chosen.description}</span><span>{formatBytes(chosen.bytes)} · {chosen.installed?'Đã tải':`Chưa tải · cần tải ${formatBytes(chosen.downloadBytes)}`}</span>{!chosen.runtimeInstalled&&<span>Bộ xử lý dùng chung · {formatBytes(chosen.runtimeBytes)}</span>}</div>}
    <p className="muted small">Bấm chạy để tải model đã chọn. Cần GPU Vulkan.</p>
   </>:<p className="muted small">Giữ tỷ lệ, tối đa khung 4K. Giữ ảnh gốc.</p>}
   <details open={initialIds.length>0}><summary>Chọn từng ảnh · {selected.length}/50</summary><div className="lan-selection">{available.map(item=><label className="checkbox-row" key={item.id}><input type="checkbox" checked={selected.includes(item.id)} disabled={busy||(!selected.includes(item.id)&&selected.length>=50)} onChange={event=>setSelected(previous=>event.target.checked?[...previous,item.id]:previous.filter(id=>id!==item.id))}/><span>{item.name}</span></label>)}</div></details>
   <label className="checkbox-row"><input type="checkbox" checked={apply} disabled={busy} onChange={event=>setApply(event.target.checked)}/>Áp dụng khi xong</label>
   {!isNative()&&<p className="inline-note">Cần bản cài Wallta.</p>}
   <div className="dialog-footer"><button className="button secondary" disabled={busy||!isNative()||!selected.length||(mode==='ai'&&!chosen)} onClick={()=>void start(false)}>Chạy ảnh đã chọn</button><button className="button primary" disabled={busy||!isNative()||!sources.some(source=>source.enabled)||(mode==='ai'&&!chosen)} onClick={()=>void start(true)}>Chạy tất cả</button></div>
  </>}
  {(error||status.error)&&<p className="form-error" role="alert">{error||status.error}</p>}
 </dialog>;
}
