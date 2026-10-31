import {Client,type ClientOptions,type AuthStrategy} from 'whatsapp-web.js';

/** Keep SDK lifecycle calls compatible with our security-pinned Puppeteer 25. */
export class WhatsAppClient extends Client{
 private readonly ownedAuth:AuthStrategy;
 constructor(options:ClientOptions & {authStrategy:AuthStrategy}){super(options);this.ownedAuth=options.authStrategy;}
 override async destroy(){await this.pupBrowser?.close();await this.ownedAuth.destroy();}
 override async logout(){
  if(!this.pupPage||!this.pupBrowser)throw Error('WhatsApp browser is not initialized');
  await this.pupPage.evaluate('window.require("WAWebSocketModel").Socket.logout()');
  await this.pupBrowser.close();
  await this.ownedAuth.logout();
 }
}
