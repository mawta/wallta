// @vitest-environment jsdom
import {act} from 'react';import {createRoot} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {PhotoPreview} from './PhotoPreview';import {callNative} from '../lib/native';import {localeStore} from '../lib/i18n';
vi.mock('../lib/native',()=>({isNative:()=>true,callNative:vi.fn(),imageUrl:(s:string)=>s,errorMessage:(e:unknown)=>String(e)}));
let host:HTMLDivElement,root:ReturnType<typeof createRoot>;const close=vi.fn();
const items=[{id:'a',sourceId:'saved',name:'A',url:'/a.jpg'},{id:'b',sourceId:'saved',name:'B',url:'/b.jpg'}];
beforeEach(()=>{Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});localeStore.setPreference('en');HTMLDialogElement.prototype.showModal=function(){this.setAttribute('open','');};HTMLDialogElement.prototype.close=function(){};vi.mocked(callNative).mockReset();close.mockReset();host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
const render=async()=>{await act(async()=>root.render(<PhotoPreview items={items} initialId="a" onClose={close}/>));};
const click=async(label:string)=>{await act(async()=>[...host.querySelectorAll('button')].find(button=>button.getAttribute('aria-label')===label||button.textContent===label)!.click());};
it('shows a full native pair, preserves alignment on zoom, and changes selected image lazily',async()=>{
 vi.mocked(callNative).mockImplementation(async(_command,args)=>({url:`/${args?.id}-full.jpg`,width:3840,height:2160,before:{url:`/${args?.id}-before.jpg`,width:640,height:360}}) as never);
 await render();expect(callNative).toHaveBeenCalledTimes(1);expect(host.querySelector('input[type="range"]')).not.toBeNull();await click('100%');expect(host.querySelector<HTMLElement>('.comparison-frame')!.style.width).toBe('3840px');await click('Next photo');expect(callNative).toHaveBeenLastCalledWith('get_photo_preview',{id:'b',sourceId:'saved'});expect(host.querySelector('img[src="/b-full.jpg"]')).not.toBeNull();expect(host.querySelector('button[aria-pressed="true"]')).toBeNull();
});
it('ignores a stale native response after moving to the next photo',async()=>{
 let finish!:(value:any)=>void;vi.mocked(callNative).mockImplementation(async(_command,args)=>args?.id==='a'?new Promise(resolve=>{finish=resolve;}):{url:'/b-full.jpg',width:3840,height:2160} as never);
 await render();await click('Next photo');await act(async()=>finish({url:'/stale.jpg',width:640,height:360}));expect(host.querySelector('img[src="/stale.jpg"]')).toBeNull();expect(host.querySelector('img[src="/b-full.jpg"]')).not.toBeNull();
});
it('keeps legacy results visible without presenting an unrelated original',async()=>{
 vi.mocked(callNative).mockResolvedValue({url:'/legacy.jpg',width:3840,height:2160,comparisonUnavailable:true} as never);await render();expect(host.querySelector('img[src="/legacy.jpg"]')).not.toBeNull();expect(host.querySelector('input[type="range"]')).toBeNull();expect(host.textContent).toContain('Original unavailable');await act(async()=>host.querySelector('dialog')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true})));expect(close).toHaveBeenCalledOnce();
});
it('shows an image error instead of a placeholder pretending to be the selected photo',async()=>{
 vi.mocked(callNative).mockRejectedValue(new Error('Image unavailable'));await render();expect(host.querySelector('[role="alert"]')?.textContent).toContain('Image unavailable');expect(host.querySelector('img')).toBeNull();
});
it('lets TV D-pad reach zoom, comparison and navigation without changing photos while focusing',async()=>{
 vi.mocked(callNative).mockResolvedValue({url:'/full.jpg',width:3840,height:2160,before:{url:'/before.jpg',width:640,height:360}} as never);await render();
 const closeButton=host.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!;closeButton.focus();
 await act(async()=>closeButton.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true})));
 expect(document.activeElement?.textContent).toBe('100%');
 await act(async()=>document.activeElement!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})));
 expect(document.activeElement?.getAttribute('type')).toBe('range');
 await act(async()=>document.activeElement!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})));
 expect(document.activeElement?.getAttribute('aria-label')).toBe('Next photo');expect(callNative).toHaveBeenCalledTimes(1);
});
it('lets TV navigation reach the footer and return for result-only photos',async()=>{
 vi.mocked(callNative).mockResolvedValue({url:'/full.jpg',width:3840,height:2160,comparisonUnavailable:true} as never);await render();
 const button=host.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!;button.focus();
 await act(async()=>button.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})));
 expect(document.activeElement?.getAttribute('aria-label')).toBe('Next photo');
 await act(async()=>document.activeElement!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true})));
 expect(document.activeElement?.textContent).toBe('100%');
});
