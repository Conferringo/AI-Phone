import {languagePicker} from './language-picker.js';
import {connectionLocale,connectionLanguages,connectionLanguageNames,connectionLabels} from './connection-i18n.js';
import {setupLabels} from './setup-i18n.js';
const $=id=>document.getElementById(id),invoke=(command,args)=>window.__TAURI_INTERNALS__.invoke(command,args);
let savedLanguage='';try{savedLanguage=localStorage.getItem('connection-language')||'';}catch{}
let locale=connectionLocale(savedLanguage||navigator.language),mode='local',edited=false,busy=false,lastError='',hostError='',snapshot=null,disposed=false,loadedResources=false,currentView='',transition=null;
const resources={phone:{cpus:2,memory_mb:2048,storage_gb:16},desktop:{cpus:2,memory_mb:2560}},pickers=[];
const errorKeys=['Cannot reach the server. Start your deployment, then retry.','Invalid server address','Use an HTTP(S) server address without credentials or a path','This preview connects only to localhost or a private LAN address','Connection already in progress'];
const errors={host_resources:'hostResources',host_backend_pending:'hostPending',installation_cancelled:'cancelled',command_line_tools:'commandLineTools',invalid_payload:'invalidPayload',download_failed:'downloadFailed',download_checksum:'invalidPayload',installation_interrupted:'interrupted',invalid_install_checkpoint:'invalidCheckpoint',checkpoint_write_failed:'checkpointFailed',foreign_installation:'foreignInstallation',linux_distribution:'linuxDistribution',linux_authorization:'linuxAuthorization',virtualization_unavailable:'virtualizationUnavailable'};
function label(key){return setupLabels[locale][key]||setupLabels.en[key]||key;}
function showPanel(){
 const view=mode==='connection'?'connection':snapshot?.progress.state?'progress':'local-setup';
 if(view===currentView)return;
 if(currentView&&!matchMedia('(prefers-reduced-motion: reduce)').matches){transition?.cancel();transition=$(view).animate([{opacity:0,transform:'translateY(7px)'},{opacity:1,transform:'none'}],{duration:220,easing:'cubic-bezier(.2,.8,.2,1)'});}
 currentView=view;
}
function render(){
 document.documentElement.lang=locale;document.documentElement.dir=locale==='ar'?'rtl':'ltr';document.querySelectorAll('[data-i18n]').forEach(node=>node.textContent=label(node.dataset.i18n));
 $('title').textContent=mode==='local'?label('title'):connectionLabels[locale][0];$('local-panel').hidden=mode!=='local';$('connection').hidden=mode!=='connection';
 $('label').textContent=connectionLabels[locale][1];$('submit').textContent=connectionLabels[locale][busy?3:2];const index=errorKeys.indexOf(lastError);$('error').textContent=index<0?lastError:connectionLabels[locale][index+4];$('error').hidden=!lastError;
 $('host-error').textContent=hostError?label(errors[hostError]||'failed'):'';$('host-error').hidden=!hostError;
 if(!snapshot){showPanel();return;}
 const {host,checks,progress}=snapshot,active=['running','license'].includes(progress.state),shown=!!progress.state;
 $('host-name').textContent=`${{macos:'macOS',windows:'Windows',linux:'Linux',android:'Android'}[host.platform]||host.platform}${host.platform==='macos'&&host.arch==='aarch64'?' · Apple Silicon':''}`;
 $('host-state').textContent='';
 $('checks').replaceChildren(...checks.map(check=>{const node=document.createElement('span');node.className=check.passed?'passed':'failed';node.textContent=`${label(check.name==='memory'?'memoryCheck':'storageCheck')} ${check.value} GB${check.passed?'':` / ${check.required} GB`}`;return node;}));
 if(!host.implemented){$('host-error').textContent=label('hostPending');$('host-error').hidden=false;}
 else if(checks.some(check=>!check.passed)&&!active&&!progress.error){$('host-error').textContent=label('hostResources');$('host-error').hidden=false;}
 $('install').disabled=!host.implemented||checks.some(check=>!check.passed)||busy;
 $('local-setup').hidden=shown;$('progress').hidden=!shown;$('phase').textContent=label(progress.phase==='failed'?(progress.state==='cancelled'?'stopped':'installFailed'):progress.phase);
 $('progress').classList.toggle('working',active);$('progress-error').hidden=!['failed','blocked'].includes(progress.state);$('progress-error').textContent=label(errors[progress.error]||(progress.state==='cancelled'?'cancelled':'failed'));
 $('license').hidden=progress.state!=='license';const license=$('license').querySelector('pre');if(license.textContent!==(progress.license||''))license.textContent=progress.license||'';$('accept').disabled=busy;
 $('retry').hidden=!['failed','blocked','cancelled','interrupted','installed'].includes(progress.state);$('retry').textContent=label(progress.state==='installed'?'start':'retry');$('cancel').hidden=!active;$('enter').hidden=progress.state!=='ready';$('existing').hidden=active;
 pickers.forEach(picker=>picker.button.disabled=active);
 showPanel();
}
const picker=languagePicker({root:$('language'),values:connectionLanguages,names:connectionLanguageNames,value:locale,onChange:value=>{locale=value;try{localStorage.setItem('connection-language',locale);}catch{}render();picker.update(locale);}});
function resourcePicker(section,key,values){
 const row=document.createElement('div');row.className='resource-row';const title=document.createElement('label');title.dataset.i18n=key==='cpus'?'cpu':key==='memory_mb'?'memory':'storage';
 const root=document.createElement('div');root.className='language-picker resource-picker';const id=`${section}-${key}`;
 root.innerHTML=`<button type="button" id="${id}" role="combobox" aria-expanded="false" aria-haspopup="listbox" aria-controls="${id}-options"><span></span><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg></button><div id="${id}-options" role="listbox" hidden></div>`;
 title.htmlFor=id;root.querySelector('button').setAttribute('aria-labelledby',id+'-label');title.id=id+'-label';
 const names=values.map(value=>key==='memory_mb'?`${value/1024} GB`:key==='storage_gb'?`${value} GB`:String(value));
 const control=languagePicker({root,values,names,value:resources[section][key],onChange:value=>{resources[section][key]=value;control.update(value);}});pickers.push({button:root.querySelector('button'),control,section,key});row.append(title,root);$(section+'-fields').append(row);
}
resourcePicker('phone','cpus',[1,2,4]);resourcePicker('phone','memory_mb',[1536,2048,3072,4096]);resourcePicker('phone','storage_gb',[16,24,32]);resourcePicker('desktop','cpus',[1,2,4]);resourcePicker('desktop','memory_mb',[2048,2560,3072,4096]);
$('server').addEventListener('input',()=>{edited=true;$('server').removeAttribute('aria-invalid');lastError='';render();});
invoke('current_server').then(value=>{if(!edited)$('server').value=value;}).catch(reason=>{lastError=String(reason);render();});
$('existing').addEventListener('click',()=>{mode='connection';render();$('server').focus();});$('back').addEventListener('click',()=>{mode='local';render();$('existing').focus();});
async function connect(serverUrl){if(busy)return;busy=true;$('submit').disabled=true;$('server').disabled=true;lastError='';render();try{await invoke('connect_server',{serverUrl,locale});}catch(reason){lastError=String(reason);$('server').setAttribute('aria-invalid','true');}finally{busy=false;$('submit').disabled=false;$('server').disabled=false;render();}}
$('connection').addEventListener('submit',event=>{event.preventDefault();void connect($('server').value);});
async function start(){if(busy)return;busy=true;hostError='';render();try{await invoke('start_deployment',{request:{locale,...resources}});}catch(error){hostError=String(error);}finally{busy=false;await refresh();}}
$('local-setup').addEventListener('submit',event=>{event.preventDefault();void start();});$('retry').addEventListener('click',start);
$('cancel').addEventListener('click',async()=>{try{await invoke('cancel_deployment');}catch(error){hostError=String(error);}await refresh();});
$('accept').addEventListener('click',async()=>{if(busy)return;busy=true;render();try{await invoke('accept_deployment_license');}catch(error){hostError=String(error);}finally{busy=false;await refresh();}});
$('enter').addEventListener('click',async()=>{await connect('http://127.0.0.1:17860');if(lastError){mode='connection';render();}});
async function refresh(){try{snapshot=await invoke('deployment_status');if(!loadedResources){loadedResources=true;if(snapshot.previous_request){for(const section of ['phone','desktop'])Object.assign(resources[section],snapshot.previous_request[section]);pickers.forEach(({control,section,key})=>control.update(resources[section][key]));}}}catch(error){hostError=String(error);}render();}
async function poll(){await refresh();if(!disposed)setTimeout(poll,['running','license'].includes(snapshot?.progress.state)?800:3000);}
window.addEventListener('pagehide',()=>{disposed=true;});render();void poll();

// Measure content rather than the viewport to avoid a resize feedback loop.
let fitFrame=0;
const fit=()=>{cancelAnimationFrame(fitFrame);fitFrame=requestAnimationFrame(()=>{const panel=document.querySelector('.setup'),main=document.querySelector('main'),style=getComputedStyle(main);const height=Math.ceil(panel.getBoundingClientRect().height+parseFloat(style.paddingTop)+parseFloat(style.paddingBottom));void invoke('fit_setup',{height}).catch(()=>{});});};
const sizeObserver=new ResizeObserver(fit);sizeObserver.observe(document.querySelector('.setup'));
window.addEventListener('pagehide',()=>{sizeObserver.disconnect();cancelAnimationFrame(fitFrame);});
