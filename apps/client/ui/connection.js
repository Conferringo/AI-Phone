import {languagePicker} from './language-picker.js';
import {connectionLocale,connectionLanguages,connectionLanguageNames,connectionLabels} from './connection-i18n.js';
const form=document.getElementById('connection'),field=document.getElementById('server'),button=document.getElementById('submit'),error=document.getElementById('error'),language=document.getElementById('language');
let savedLanguage='';try{savedLanguage=localStorage.getItem('connection-language')||'';}catch{}
let locale=connectionLocale(savedLanguage||navigator.language),edited=false,busy=false,lastError='';
const invoke=(command,args)=>window.__TAURI_INTERNALS__.invoke(command,args);
const errorKeys=['Cannot reach the server. Start your deployment, then retry.','Invalid server address','Use an HTTP(S) server address without credentials or a path','This preview connects only to localhost or a private LAN address','Connection already in progress'];
function render(){
 const labels=connectionLabels[locale];document.documentElement.lang=locale;
 document.getElementById('title').textContent=labels[0];document.getElementById('label').textContent=labels[1];button.textContent=labels[busy?3:2];
 const index=errorKeys.indexOf(lastError);error.textContent=index<0?lastError:labels[index+4];error.hidden=!lastError;
}
const picker=languagePicker({root:language,values:connectionLanguages,names:connectionLanguageNames,value:locale,onChange:value=>{locale=value;try{localStorage.setItem('connection-language',locale);}catch{}render();picker.update(locale);}});
field.addEventListener('input',()=>{edited=true;field.removeAttribute('aria-invalid');lastError='';render();});
invoke('current_server').then(value=>{if(!edited)field.value=value;}).catch(reason=>{lastError=String(reason);render();});
form.addEventListener('submit',async event=>{event.preventDefault();if(busy)return;busy=true;button.disabled=true;field.disabled=true;lastError='';render();
 try{await invoke('connect_server',{serverUrl:field.value,locale});}
 catch(reason){lastError=String(reason);field.setAttribute('aria-invalid','true');}
 finally{busy=false;button.disabled=false;field.disabled=false;render();if(lastError)field.focus();}
});
render();
