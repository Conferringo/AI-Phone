import PhoneConsole from "./phone-console";
import type { Metadata } from "next";
import { cookies } from "next/headers";
import { SESSION_COOKIE,validSession } from "@/lib/session-server";
import {deviceDefaults} from '@/lib/device-defaults-server';
import {SessionBootstrap} from '@/components/session-bootstrap';
export const metadata: Metadata = {title:"AI Phone"};
export default async function PhonePage(){
 const session=await validSession((await cookies()).get(SESSION_COOKIE)?.value??'');
 if(session!==true)return <SessionBootstrap/>;
 return <PhoneConsole defaults={await deviceDefaults()}/>;
}
