import { X } from 'lucide-react';
import { formatBytes, type UpscaleController } from '../lib/upscale';
export function UpscaleProgress({controller,onResults}:{controller:UpscaleController;onResults:()=>void}){
 const {status,active,busy,error}=controller;if(!status.jobId&&status.phase==='idle')return null;
 const downloading=status.phase==='downloading';const model=controller.models.find(item=>item.id===status.model)?.name;
 const title=active?downloading?`Tải ${model||'model AI'}`:`4K${model?` · ${model}`:''} · ${status.processed}/${status.total}`:`Đã lưu ${status.completed} ảnh${status.phase==='cancelled'?' · Đã hủy':status.phase==='error'?' · Đã dừng':''}`;
 return <aside className="upscale-bottom" aria-label="Tiến trình upscale">
  <div className="upscale-bottom-info" role="status"><strong>{title}</strong><span>{downloading?`${formatBytes(status.downloadedBytes)} / ${formatBytes(status.totalDownloadBytes)}`:status.failed?`${status.failed} lỗi`:status.message||''}</span></div>
  {active&&<progress aria-label={downloading?'Tải model':'Xử lý ảnh'} max={Math.max(1,downloading?status.totalDownloadBytes:status.total)} value={downloading?status.downloadedBytes:status.processed}/>}
  <div className="upscale-bottom-actions">{active?<button className="button secondary" disabled={busy} onClick={()=>void controller.cancel()}>Hủy</button>:<>{status.results.length>0&&<button className="button secondary" onClick={onResults}>Xem ảnh</button>}{status.source&&<button className="button primary" disabled={busy} onClick={()=>void controller.apply()}>Áp dụng</button>}<button className="icon-button" aria-label="Ẩn tiến trình" onClick={controller.dismiss}><X size={18}/></button></>}</div>
  {(error||status.error)&&<p className="upscale-bottom-error" role="alert">{error||status.error}</p>}
 </aside>;
}
