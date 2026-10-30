import type { Source } from './types';
export type UpscaleMode = 'fast' | 'ai';
export type ModelId = 'photo' | 'anime' | 'anime-light';
export interface UpscaleModel { id:ModelId; name:string; description:string; bytes:number; installed:boolean; downloadBytes:number; runtimeBytes:number; runtimeInstalled:boolean }
export interface UpscaleResult { id:string; name:string; width:number; height:number; originalPreview:string; resultPreview:string }
export interface UpscaleStatus {
 jobId:string|null; phase:'idle'|'downloading'|'processing'|'ready'|'cancelled'|'error'; mode:UpscaleMode; model?:ModelId|null;
 completed:number; processed:number; failed:number; total:number; downloadedBytes:number; totalDownloadBytes:number;
 results:UpscaleResult[]; source?:Source|null; error?:string|null; message?:string|null;
}
export interface UpscaleCollection {id:string;name:string;model?:string|null;createdAt?:string|null;count:number;bytes:number;sourceId?:string|null}
export interface UpscaleStorage {cacheBytes:number;collections:UpscaleCollection[]}
export interface UpscaleRequest {all?:boolean;sourceId?:string|null;ids?:string[];mode:UpscaleMode;model?:ModelId;apply:boolean}
export interface UpscaleController {
 status:UpscaleStatus; active:boolean; busy:boolean; error:string; models:UpscaleModel[];
 start:(request:UpscaleRequest)=>Promise<boolean>;loadModels:()=>Promise<void>;cancel:()=>Promise<void>;
 apply:()=>Promise<boolean>;dismiss:()=>void;
}
export const idleUpscale:UpscaleStatus={jobId:null,phase:'idle',mode:'fast',completed:0,processed:0,failed:0,total:0,downloadedBytes:0,totalDownloadBytes:0,results:[]};
export const upscaleActive=(status:UpscaleStatus)=>status.phase==='processing'||status.phase==='downloading';
export const formatBytes=(bytes:number)=>bytes>=1e9?`${(bytes/1e9).toFixed(2)} GB`:bytes>=1e6?`${(bytes/1e6).toFixed(2)} MB`:bytes>=1000?`${(bytes/1000).toFixed(1)} KB`:`${bytes} B`;
