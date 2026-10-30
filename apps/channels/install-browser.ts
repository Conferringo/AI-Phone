// Development/bootstrap installer. Product distributions must ship this component prebuilt.
import {Browser,detectBrowserPlatform,resolveBuildId,install} from '@puppeteer/browsers';
import {mkdirSync,existsSync,writeFileSync,renameSync} from 'node:fs';
import {resolve,relative} from 'node:path';
import {whatsappBrowser} from './whatsapp-browser';
const root=process.env.AI_PHONE_PROJECT_ROOT||resolve(import.meta.dir,'../..'),cache=resolve(root,'.tools/whatsapp-browser');
if(process.argv.includes('--verify')){whatsappBrowser(root);}
else if(existsSync(resolve(cache,'browser.json'))){whatsappBrowser(root);console.log('WhatsApp browser component ready');}
else{
 const platform=detectBrowserPlatform();if(!platform||platform==='linux_arm')throw Error('No matching Chromium build for this host; a bundled platform runtime is required');
 mkdirSync(cache,{recursive:true,mode:0o700});
 const buildId=await resolveBuildId(Browser.CHROMIUM,platform,'latest');
 const browser=await install({browser:Browser.CHROMIUM,platform,buildId,cacheDir:cache});
 const source=`https://storage.googleapis.com/chromium-browser-snapshots/`;
 const manifest={format:1,browser:'chromium',platform,buildId,source,executable:relative(cache,browser.executablePath)};
 const temp=resolve(cache,'browser.json.tmp');writeFileSync(temp,JSON.stringify(manifest,null,2)+'\n',{mode:0o600});renameSync(temp,resolve(cache,'browser.json'));
 console.log('WhatsApp Chromium installed',platform,buildId);
}
