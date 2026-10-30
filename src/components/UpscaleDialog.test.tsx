// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { UpscaleDialog } from './UpscaleDialog';
import { callNative } from '../lib/native';
vi.mock('../lib/native', () => ({ isNative: () => true, callNative: vi.fn(), imageUrl: (s: string) => s, errorMessage: (e: unknown) => String(e) }));
let host: HTMLDivElement; let root: ReturnType<typeof createRoot>;
const media = Array.from({length: 75}, (_, i) => ({id: `img${i}`, name: `Image ${i}`, sourceId:'s', url:`/original${i}`}));
const source = {id:'s',name:'Drive photos',kind:'drive' as const,location:'folder',enabled:true,recursive:false};
const models = [
 {id:'photo',name:'Ảnh thật',description:'RealESRGAN x4plus',bytes:33540549,installed:true,downloadBytes:0,runtimeBytes:6344112,runtimeInstalled:true},
 {id:'anime',name:'Anime / Minh họa',description:'RealESRGAN Anime 6B',bytes:8973790,installed:false,downloadBytes:8973790,runtimeBytes:6344112,runtimeInstalled:true},
];
const idle = {jobId:null,phase:'idle',mode:'fast',model:null,completed:0,processed:0,failed:0,total:0,downloadedBytes:0,totalDownloadBytes:0,results:[]};
let controller: any;
const onClose=vi.fn();
beforeEach(() => {
 Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});
 HTMLDialogElement.prototype.showModal=function(){}; HTMLDialogElement.prototype.close=function(){};
 vi.mocked(callNative).mockReset().mockResolvedValue(idle as never); onClose.mockClear();
 controller={status:idle,active:false,busy:false,error:'',models,start:vi.fn().mockResolvedValue(true),loadModels:vi.fn().mockResolvedValue(undefined),cancel:vi.fn(),apply:vi.fn(),dismiss:vi.fn()};
 host=document.createElement('div');document.body.append(host);root=createRoot(host);
});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
const render=async(props:Record<string,unknown>={})=>{await act(async()=>root.render(<UpscaleDialog {...({platform:'windows',sources:[source],media,controller,onClose,...props} as any)} />));};
const click=async(text:string)=>{const button=[...host.querySelectorAll('button')].find(b=>b.textContent?.includes(text)||b.getAttribute('aria-label')===text);expect(button).toBeDefined();await act(async()=>button!.click());};
const choose=async(label:string,value:string)=>{const element=host.querySelector(`[aria-label="${label}"]`) as HTMLSelectElement;expect(element).not.toBeNull();await act(async()=>{element.value=value;element.dispatchEvent(new Event('change',{bubbles:true}));});};
it('runs all from a source without clipping the source library to fifty IDs',async()=>{
 await render(); await choose('Nguồn upscale','s');await click('Chạy tất cả');
 expect(controller.start).toHaveBeenCalledWith({all:true,sourceId:'s',mode:'fast',apply:false});expect(onClose).toHaveBeenCalledOnce();
});
it('passes only the selected model and displays its size and download state',async()=>{
 await render(); await choose('Chế độ upscale','ai'); await choose('Model upscale','anime');
 expect(host.textContent).toContain('8.97 MB');expect(host.textContent).toContain('Chưa tải');
 expect(controller.start).not.toHaveBeenCalled();await click('Chạy tất cả');
 expect(controller.start).toHaveBeenCalledWith({all:true,sourceId:null,mode:'ai',model:'anime',apply:false});
});
it('shows separate shared-runtime bytes if they are still needed',async()=>{
 controller.models=models.map(m=>({...m,runtimeInstalled:false}));await render();await choose('Chế độ upscale','ai');
 expect(host.textContent).toContain('Bộ xử lý');expect(host.textContent).toContain('6.34 MB');
});
it('closing configuration keeps the job running and the dialog contains no duplicate progress',async()=>{
 controller.status={...idle,jobId:'job',phase:'processing',total:75};controller.active=true;
 await render();expect(host.querySelectorAll('progress')).toHaveLength(0);await click('Đóng');
 expect(controller.cancel).not.toHaveBeenCalled();expect(onClose).toHaveBeenCalledOnce();
});
it('keeps manual image selection and sends selected IDs rather than All',async()=>{
 await render({initialIds:['img2']});await click('Chạy ảnh đã chọn');
 expect(controller.start).toHaveBeenCalledWith({ids:['img2'],mode:'fast',apply:false});
});
it('shows saved real previews after a partial failure without requiring another save',async()=>{
 controller.status={...idle,jobId:'job',phase:'error',completed:1,processed:2,total:3,failed:1,source:{...source,id:'result'},error:'Image failed',results:[{id:'img0',name:'Image',width:3840,height:2160,originalPreview:'/before.jpg',resultPreview:'/after.jpg'}]};
 await render();expect(host.querySelector('img[alt="Sau"]')?.getAttribute('src')).toBe('/after.jpg');expect(host.textContent).toContain('Đã lưu');
 expect([...host.querySelectorAll('button')].some(b=>b.textContent?.includes('Lưu ảnh 4K'))).toBe(false);
});
it('does not offer AI on Android TV',async()=>{await render({platform:'android-tv'});expect(host.querySelector('option[value="ai"]')?.hasAttribute('disabled')).toBe(true);});
