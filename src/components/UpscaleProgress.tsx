import { t } from '../lib/i18n';
import { X } from 'lucide-react';
import { formatBytes, type UpscaleController } from '../lib/upscale';
export function UpscaleProgress({controller,onResults}:{controller:UpscaleController;onResults:()=>void}){
 const {status,active,busy,error}=controller;if(!status.jobId&&status.phase==='idle')return null;
 const downloading=status.phase==='downloading';const model=controller.models.find(item=>item.id===status.model)?.name;
 const title=active?downloading?t("Tải {0}", { 0: model||'model AI' }):`4K${model?` · ${model}`:''} · ${status.processed}/${status.total}`:t("Đã lưu {0} ảnh{1}", { 0: status.completed, 1: status.phase==='cancelled'?t(" · Đã hủy"):status.phase==='error'?t(" · Đã dừng"):'' });
 return <aside className="upscale-bottom" aria-label={t("Tiến trình upscale")}>
  <div className="upscale-bottom-info" role="status"><strong>{title}</strong><span>{downloading?`${formatBytes(status.downloadedBytes)} / ${formatBytes(status.totalDownloadBytes)}`:status.failed?t("{0} lỗi", { 0: status.failed }):t(status.message||'')}</span></div>
  {active&&<progress aria-label={downloading?t("Tải model"):t("Xử lý ảnh")} max={Math.max(1,downloading?status.totalDownloadBytes:status.total)} value={downloading?status.downloadedBytes:status.processed}/>}
  <div className="upscale-bottom-actions">{active?<button className="button secondary" disabled={busy} onClick={()=>void controller.cancel()}>{t("Hủy")}</button>:<>{status.results.length>0&&<button className="button secondary" onClick={onResults}>{t("Xem ảnh")}</button>}{status.source&&<button className="button primary" disabled={busy} onClick={()=>void controller.apply()}>{t("Áp dụng")}</button>}<button className="icon-button" aria-label={t("Ẩn tiến trình")} onClick={controller.dismiss}><X size={18}/></button></>}</div>
  {(error||status.error)&&<p className="upscale-bottom-error" role="alert">{t(error || status?.error || '')}</p>}
 </aside>;
}
