import { t } from './i18n';
import {callNative} from './native';
import type {AppState,Source} from './types';
export async function applyUpscale(source:Source):Promise<void>{
 const fresh=await callNative<AppState>('get_state');
 const current=fresh.sources.find(item=>item.id===source.id);
 if(!current)throw new Error(t("Thư mục ảnh 4K đã bị xóa."));
 if(!current.enabled)await callNative('save_state',{state:{...fresh,sources:fresh.sources.map(item=>item.id===source.id?{...item,enabled:true}:item)}});
 await callNative('list_media');
 await callNative('next_wallpaper',{sourceId:source.id,startRunning:true});
}
