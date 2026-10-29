/** The connection page owns its DOM; keyboard navigation follows the listbox pattern. */
export function languagePicker({root,values,names,value,onChange}){
 const trigger=root.querySelector('[role=combobox]'),list=root.querySelector('[role=listbox]');
 let selected=value,prefix='',typedAt=0;
 const options=values.map((value,index)=>{const button=document.createElement('button');button.type='button';button.role='option';button.tabIndex=-1;button.textContent=names[index];button.addEventListener('click',()=>{onChange(value);close(true);});list.append(button);return button;});
 function update(value){selected=value;trigger.querySelector('span').textContent=names[values.indexOf(value)];options.forEach((option,i)=>option.setAttribute('aria-selected',String(values[i]===value)));}
 function close(focus=false){list.hidden=true;trigger.setAttribute('aria-expanded','false');if(focus)trigger.focus();}
 function open(){list.hidden=false;trigger.setAttribute('aria-expanded','true');options[values.indexOf(selected)]?.focus();}
 trigger.addEventListener('click',()=>list.hidden?open():close());
 root.addEventListener('keydown',event=>{
  if(event.key==='Escape'){if(!list.hidden){event.preventDefault();event.stopPropagation();close(true);}return;}
  if(event.key==='Tab'){close();return;}
  if(event.key==='ArrowDown'||event.key==='ArrowUp'){
   event.preventDefault();if(list.hidden){open();return;}
   const current=options.indexOf(document.activeElement),direction=event.key==='ArrowDown'?1:-1;
   options[(current+direction+options.length)%options.length].focus();return;
  }
  if(!list.hidden&&(event.key==='Home'||event.key==='End')){event.preventDefault();options[event.key==='Home'?0:options.length-1].focus();return;}
  if(!list.hidden&&event.key.length===1&&event.key!==' '&&!event.metaKey&&!event.ctrlKey){
   event.preventDefault();prefix=Date.now()-typedAt<600?prefix+event.key:event.key;typedAt=Date.now();
   options.find(option=>option.textContent.toLocaleLowerCase().startsWith(prefix.toLocaleLowerCase()))?.focus();
  }
 });
 const outside=event=>{if(!root.contains(event.target))close();};document.addEventListener('pointerdown',outside);
 const blur=event=>{if(!root.contains(event.relatedTarget))close();};root.addEventListener('focusout',blur);
 update(value);return {update,dispose(){document.removeEventListener('pointerdown',outside);root.removeEventListener('focusout',blur);}};
}
