import { t, localeTag } from '../lib/i18n';
import { useEffect, useState } from 'react';
import { Folder, Trash2 } from 'lucide-react';
import { callNative, errorMessage, isNative } from '../lib/native';
import { formatBytes, type UpscaleCollection, type UpscaleStorage as Storage } from '../lib/upscale';
export function UpscaleStorage({active,onChanged}:{active:boolean;onChanged:()=>unknown}){
 const [storage,setStorage]=useState<Storage>({cacheBytes:0,collections:[]});const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [deleting,setDeleting]=useState<UpscaleCollection|null>(null);
 useEffect(()=>{let disposed=false;if(isNative())void callNative<Storage>('upscale_storage').then(value=>{if(!disposed&&value)setStorage(value);}).catch(e=>{if(!disposed)setError(errorMessage(e));});return()=>{disposed=true;};},[active]);
 const run=async(command:string,args?:Record<string,unknown>)=>{if(busy||active)return;setBusy(true);setError('');try{setStorage(await (args?callNative<Storage>(command,args):callNative<Storage>(command)));setDeleting(null);await onChanged();}catch(e){setError(errorMessage(e));}finally{setBusy(false);}};
 const disabled=busy||active||!isNative();
 return <div className="upscale-storage">
  <div className="setting-row"><div><strong>Cache · {formatBytes(storage.cacheBytes)}</strong><p>{t("Giữ ảnh đã lưu và model AI.")}</p></div><button className="button secondary" disabled={disabled} onClick={()=>void run('upscale_clear_cache')}><Trash2 size={15}/>{t("Dọn cache")}</button></div>
  <h4>{t("Thư mục ảnh 4K")}</h4>
  {storage.collections.length?storage.collections.map(collection=><div className="upscale-collection" key={collection.id}><Folder size={18}/><div><strong>{collection.name}</strong><small>{collection.count} {t("ảnh ·")} {formatBytes(collection.bytes)}{collection.model?` · ${collection.model}`:''}{collection.createdAt?` · ${new Date(collection.createdAt).toLocaleDateString(localeTag())}`:''}</small></div><button className="icon-button danger-text" aria-label={t("Xóa {0}", { 0: collection.name })} disabled={disabled} onClick={()=>setDeleting(collection)}><Trash2 size={16}/></button></div>):<p className="muted small">{t("Chưa có ảnh đã upscale.")}</p>}
  {error&&<p className="form-error" role="alert">{t(error)}</p>}
  {deleting&&<div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="delete-upscale-title"><div className="confirm-dialog"><h2 id="delete-upscale-title">{t("Xóa “")}{deleting.name}”?</h2><p>{deleting.count} {t("ảnh đã upscale ·")} {formatBytes(deleting.bytes)}{t(". Giữ ảnh gốc.")}</p><div className="dialog-footer"><button className="button secondary" disabled={busy} onClick={()=>setDeleting(null)}>{t("Giữ lại")}</button><button className="button danger" disabled={disabled} onClick={()=>void run('upscale_delete_collection',{collectionId:deleting.id})}>{t("Xóa thư mục")}</button></div></div></div>}
 </div>;
}
