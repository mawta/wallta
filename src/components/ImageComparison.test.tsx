// @vitest-environment jsdom
import {act} from 'react';
import {createRoot} from 'react-dom/client';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {ImageComparison} from './ImageComparison';
import {localeStore} from '../lib/i18n';
let host:HTMLDivElement, root:ReturnType<typeof createRoot>;
beforeEach(()=>{Object.assign(globalThis,{IS_REACT_ACT_ENVIRONMENT:true});localeStore.setPreference('en');host=document.createElement('div');document.body.append(host);root=createRoot(host);});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();});
const render=async(actualSize=false)=>{await act(async()=>root.render(<ImageComparison afterUrl="/full.jpg" beforeUrl="/original.jpg" width={3840} height={2160} actualSize={actualSize}/>));};
const slider=()=>host.querySelector<HTMLInputElement>('input[type="range"]')!;
const key=async(value:string)=>{await act(async()=>slider().dispatchEvent(new KeyboardEvent('keydown',{key:value,bubbles:true})));};
it('shares a canvas and exposes keyboard and endpoint control of the original overlay',async()=>{
 await render();expect(slider().value).toBe('50');await key('ArrowRight');expect(slider().value).toBe('51');await key('End');expect(slider().value).toBe('100');expect((host.querySelector('.comparison-before') as HTMLElement).style.clipPath).toBe('inset(0 0% 0 0)');await key('Home');expect(slider().value).toBe('0');
 const images=host.querySelectorAll('.comparison-image');expect(images).toHaveLength(2);expect(images[0].parentElement).toBe(images[1].parentElement);
});
it('keeps the divider position when toggling shared 100% zoom',async()=>{
 await render();await key('End');await render(true);expect(slider().value).toBe('100');const frame=host.querySelector<HTMLElement>('.comparison-frame')!;expect(frame.style.width).toBe('3840px');expect(frame.style.height).toBe('2160px');
});
it('uses pointer geometry for dragging and stops on cancellation',async()=>{
 await render();const frame=host.querySelector<HTMLElement>('.comparison-frame')!;vi.spyOn(frame,'getBoundingClientRect').mockReturnValue({left:100,width:400} as DOMRect);
 const pointer=async(type:string,x:number)=>{await act(async()=>{const event=new MouseEvent(type,{bubbles:true,clientX:x,button:0});Object.defineProperty(event,'pointerId',{value:1});slider().dispatchEvent(event);});};
 await pointer('pointerdown',200);expect(slider().value).toBe('25');await pointer('pointermove',900);expect(slider().value).toBe('100');await pointer('pointercancel',900);await pointer('pointermove',100);expect(slider().value).toBe('100');
});
it('keeps the result visible if the original image cannot load',async()=>{
 await render();await act(async()=>host.querySelector('img[alt="Before"]')!.dispatchEvent(new Event('error')));expect(slider()).toBeNull();expect(host.querySelector('img[src="/full.jpg"]')).not.toBeNull();expect(host.textContent).toContain('Original unavailable');
});
it('ignores the native touch range update while pointer geometry controls the divider',async()=>{
 await render();const frame=host.querySelector<HTMLElement>('.comparison-frame')!;vi.spyOn(frame,'getBoundingClientRect').mockReturnValue({left:0,width:400} as DOMRect);
 await act(async()=>slider().dispatchEvent(new MouseEvent('pointerdown',{bubbles:true,clientX:100,button:0})));
 await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')!.set!.call(slider(),'22');slider().dispatchEvent(new Event('input',{bubbles:true}));});
 expect(slider().value).toBe('25');
});
