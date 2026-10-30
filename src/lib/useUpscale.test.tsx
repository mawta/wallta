// @vitest-environment jsdom
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useUpscale } from './useUpscale';
import { callNative } from './native';
import type { UpscaleStatus } from './upscale';
vi.mock('./native',()=>({isNative:()=>true,callNative:vi.fn(),errorMessage:(e:unknown)=>String(e)}));
let host:HTMLDivElement; let root:ReturnType<typeof createRoot>; let controller:ReturnType<typeof useUpscale>;
const idle:UpscaleStatus={jobId:null,phase:'idle',mode:'fast',completed:0,processed:0,failed:0,total:0,downloadedBytes:0,totalDownloadBytes:0,results:[]};
const processing:UpscaleStatus={...idle,jobId:'j',phase:'processing',total:75};
let nativeStatus=idle;const applied=vi.fn();const changed=vi.fn();
function Harness(){controller=useUpscale(true,{onApply:applied,onChanged:changed});return null;}
beforeEach(()=>{Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});vi.useFakeTimers();nativeStatus=idle;applied.mockReset().mockResolvedValue(true);changed.mockReset();vi.mocked(callNative).mockReset().mockImplementation(async(command)=>{
 if(command==='upscale_status')return nativeStatus as never;
 if(command==='upscale_models')return [] as never;
 if(command==='upscale_start_all'||command==='upscale_start'){nativeStatus=processing;return processing as never;}
 if(command==='upscale_cancel')return {...processing,phase:'cancelled'} as never;
 return undefined as never;
});host=document.createElement('div');root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());vi.useRealTimers();});
it('starts native All without a media snapshot and keeps running after hook unmount',async()=>{
 await act(async()=>root.render(<Harness/>));await act(async()=>{await controller.start({all:true,sourceId:null,mode:'ai',model:'anime',apply:false});});
 expect(callNative).toHaveBeenCalledWith('upscale_start_all',{sourceId:null,mode:'ai',model:'anime'});
 expect(controller.active).toBe(true);await act(async()=>root.render(null));expect(callNative).not.toHaveBeenCalledWith('upscale_cancel',expect.anything());
});
it('applies once only after the native run is durable and ready',async()=>{
 await act(async()=>root.render(<Harness/>));await act(async()=>{await controller.start({all:true,mode:'fast',apply:true});});
 const source={id:'saved',name:'4K',kind:'local' as const,location:'managed',enabled:false,recursive:true};nativeStatus={...processing,phase:'ready',source} as typeof idle;
 await act(async()=>vi.advanceTimersByTimeAsync(800));expect(applied).toHaveBeenCalledExactlyOnceWith(source);
 await act(async()=>vi.advanceTimersByTimeAsync(2400));expect(applied).toHaveBeenCalledTimes(1);expect(changed).toHaveBeenCalled();
});
it('refreshes saved partial results on failure without applying them automatically',async()=>{
 await act(async()=>root.render(<Harness/>));await act(async()=>{await controller.start({all:true,mode:'fast',apply:true});});
 nativeStatus={...processing,phase:'error',completed:1,source:{id:'partial',name:'4K',kind:'local',location:'managed',enabled:false,recursive:true},error:'Disk full'} as typeof idle;
 await act(async()=>vi.advanceTimersByTimeAsync(800));expect(changed).toHaveBeenCalled();expect(applied).not.toHaveBeenCalled();expect(controller.status.phase).toBe('error');
});
it('restores an active native run without silently enabling automatic apply',async()=>{
 nativeStatus=processing;await act(async()=>root.render(<Harness/>));expect(controller.active).toBe(true);
 nativeStatus={...processing,phase:'ready',source:{id:'saved',name:'4K',kind:'local',location:'managed',enabled:false,recursive:true}} as typeof idle;
 await act(async()=>vi.advanceTimersByTimeAsync(800));expect(applied).not.toHaveBeenCalled();
});
it('blocks a new queue until automatic apply has finished',async()=>{
 let release!:()=>void;applied.mockImplementation(()=>new Promise<boolean>(resolve=>{release=()=>resolve(true);}));
 await act(async()=>root.render(<Harness/>));await act(async()=>{await controller.start({all:true,mode:'fast',apply:true});});
 nativeStatus={...processing,phase:'ready',source:{id:'saved',name:'4K',kind:'local',location:'managed',enabled:false,recursive:true}};
 await act(async()=>{vi.advanceTimersByTime(800);await Promise.resolve();await Promise.resolve();});
 expect(applied).toHaveBeenCalledOnce();expect(controller.busy).toBe(true);
 await act(async()=>{expect(await controller.start({all:true,mode:'fast',apply:false})).toBe(false);});
 await act(async()=>{release();await Promise.resolve();});expect(controller.busy).toBe(false);
});

