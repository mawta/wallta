import { t } from '../lib/i18n';
import { useEffect, useRef, useState } from 'react';
import { ImageComparison } from './ImageComparison';
import { X } from 'lucide-react';
import { callNative, imageUrl, isNative } from '../lib/native';
import type { Capabilities, MediaItem, Source } from '../lib/types';
import { formatBytes, type ModelId, type UpscaleController, type UpscaleMode, type UpscaleStorage } from '../lib/upscale';

interface Props {platform:Capabilities['platform'];sources:Source[];media:MediaItem[];initialIds?:string[];controller:UpscaleController;onClose:()=>void;onPreview?:(item:MediaItem)=>void}
export function UpscaleDialog({platform,sources,media,initialIds=[],controller,onClose,onPreview}:Props){
 const dialog=useRef<HTMLDialogElement>(null);const [mode,setMode]=useState<UpscaleMode>('fast');const [model,setModel]=useState<ModelId>('photo');
 const [sourceId,setSourceId]=useState('');const [selected,setSelected]=useState(initialIds.filter(id=>media.some(item=>item.id===id)).slice(0,50));
 const [apply,setApply]=useState(false);const [resultIndex,setResultIndex]=useState(0);const [showResults,setShowResults]=useState(controller.status.results.length>0);
 const [managedSources,setManagedSources]=useState<string[]>([]);
 const {status,active,busy,error,models}=controller;const chosen=models.find(item=>item.id===model);const result=status.results[resultIndex]||status.results[0];
 useEffect(()=>{const element=dialog.current;element?.showModal();void controller.loadModels();return()=>element?.close();},[]);
 useEffect(()=>{let disposed=false;if(isNative())void callNative<UpscaleStorage>('upscale_storage').then(value=>{if(!disposed&&Array.isArray(value?.collections)){const ids=value.collections.flatMap(item=>item.sourceId?[item.sourceId]:[]);setManagedSources(ids);setSourceId(previous=>ids.includes(previous)?'':previous);setSelected(previous=>previous.filter(id=>!ids.includes(media.find(item=>item.id===id)?.sourceId||'')));}}).catch(()=>{});return()=>{disposed=true;};},[status.jobId,status.phase,media]);
 useEffect(()=>{const id=status.source?.id;if(id){setSourceId(previous=>previous===id?'':previous);setSelected(previous=>previous.filter(item=>media.find(image=>image.id===item)?.sourceId!==id));}},[status.source?.id,media]);
 const start=async(all:boolean)=>{if(await controller.start({...(all?{all:true,sourceId:sourceId||null}:{ids:selected}),mode,...(mode==='ai'?{model}:{}),apply}))onClose();};
 const originals=sources.filter(source=>source.enabled&&source.id!==status.source?.id&&!managedSources.includes(source.id));
 const available=media.filter(item=>item.sourceId!==status.source?.id&&!managedSources.includes(item.sourceId)&&(!sourceId||item.sourceId===sourceId));
 return <dialog ref={dialog} className="source-dialog upscale-dialog" aria-labelledby="upscale-title" onCancel={event=>{event.preventDefault();onClose();}}>
  <div className="dialog-heading"><h2 id="upscale-title">{t("Nâng ảnh 4K")}</h2><button className="icon-button" aria-label={t("Đóng")} onClick={onClose}><X size={20}/></button></div>
  {active?<p className="muted small">{t("Đang chạy. Tiến trình ở cuối cửa sổ.")}</p>:showResults&&result?<>
   {status.results.length>1&&<label className="field">{t("Ảnh kết quả")}<select value={resultIndex} onChange={event=>setResultIndex(Number(event.target.value))}>{status.results.map((item,index)=><option key={item.id} value={index}>{item.name}</option>)}</select></label>}
   <div className="upscale-comparison-slider"><ImageComparison key={result.id} beforeUrl={imageUrl(result.originalPreview)} afterUrl={imageUrl(result.resultPreview)} width={result.width} height={result.height}/></div><p className="muted small">{t("Sau ·")} {result.width} × {result.height}</p>
   {result.mediaId&&result.sourceId&&onPreview&&<button className="button secondary" onClick={()=>onPreview({id:result.mediaId!,sourceId:result.sourceId!,name:result.name,url:result.resultPreview})}>{t("Xem ảnh")}</button>}
   <p className="muted small">{t("Đã lưu")} {status.completed} {t("ảnh")}{status.failed?t(" · {0} ảnh lỗi", { 0: status.failed }):''}{status.source?` · ${status.source.name}`:''}</p>
   <div className="dialog-footer"><button className="button secondary" onClick={()=>setShowResults(false)}>{t("Chạy bộ khác")}</button>{status.source&&<button className="button primary" disabled={busy} onClick={()=>void controller.apply()}>{t("Áp dụng")}</button>}</div>
  </>:<>
   <label className="field">{t("Nguồn")}<select aria-label={t("Nguồn upscale")} value={sourceId} disabled={busy} onChange={event=>{const scope=event.target.value;setSourceId(scope);setSelected(previous=>previous.filter(id=>!scope||media.find(item=>item.id===id)?.sourceId===scope));}}><option value="">{t("Tất cả nguồn đang bật")}</option>{originals.map(source=><option key={source.id} value={source.id}>{source.name}</option>)}</select></label>
   <label className="field">{t("Chế độ")}<select aria-label={t("Chế độ upscale")} value={mode} disabled={busy} onChange={event=>setMode(event.target.value as UpscaleMode)}><option value="fast">{t("Nhanh")}</option><option value="ai" disabled={platform!=='windows'}>AI</option></select></label>
   {mode==='ai'?<>
    <label className="field">Model<select aria-label={t("Model upscale")} value={model} disabled={busy||!models.length} onChange={event=>setModel(event.target.value as ModelId)}>{models.map(item=><option key={item.id} value={item.id}>{item.name} · {formatBytes(item.bytes)} · {item.installed?t("Đã tải"):t("Chưa tải")}</option>)}</select></label>
    {chosen&&<div className="model-details"><span>{t(chosen.description)}</span>{!chosen.installed&&chosen.downloadBytes!==chosen.bytes&&<span>{t("Cần tải thêm")} {formatBytes(chosen.downloadBytes)}</span>}{!chosen.runtimeInstalled&&<span>{t("Bộ xử lý dùng chung ·")} {formatBytes(chosen.runtimeBytes)}</span>}</div>}
    <p className="muted small">{t("Bấm chạy để tải model đã chọn. Cần GPU Vulkan.")}</p>
   </>:<p className="muted small">{t("Giữ tỷ lệ, tối đa khung 4K. Giữ ảnh gốc.")}</p>}
   <details open={initialIds.length>0}><summary>{t("Chọn từng ảnh ·")} {selected.length}/50</summary><div className="lan-selection">{available.map(item=><label className="checkbox-row" key={item.id}><input type="checkbox" checked={selected.includes(item.id)} disabled={busy||(!selected.includes(item.id)&&selected.length>=50)} onChange={event=>setSelected(previous=>event.target.checked?[...previous,item.id]:previous.filter(id=>id!==item.id))}/><span>{item.name}</span></label>)}</div></details>
   <label className="checkbox-row"><input type="checkbox" checked={apply} disabled={busy} onChange={event=>setApply(event.target.checked)}/>{t("Áp dụng khi xong")}</label>
   {!isNative()&&<p className="inline-note">{t("Cần bản cài Wallta.")}</p>}
   <div className="dialog-footer"><button className="button secondary" disabled={busy||!isNative()||!selected.length||(mode==='ai'&&!chosen)} onClick={()=>void start(false)}>{t("Chạy ảnh đã chọn")}</button><button className="button primary" disabled={busy||!isNative()||!originals.length||(mode==='ai'&&!chosen)} onClick={()=>void start(true)}>{t("Chạy tất cả")}</button></div>
  </>}
  {(error||status.error)&&<p className="form-error" role="alert">{t(error || status?.error || '')}</p>}
 </dialog>;
}
