import {settingsBridge} from '@/lib/settings-server';
export const runtime='nodejs';
export async function POST(request:Request,{params}:{params:Promise<{operation:string}>}){return settingsBridge(request,(await params).operation,'desktop');}
