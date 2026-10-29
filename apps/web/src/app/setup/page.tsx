import {redirect} from 'next/navigation';
import SetupForm from './setup-form';
export default function SetupPage(){if(process.env.AI_PHONE_LOCAL_ENTRY==='1')redirect('/phone');return <SetupForm/>;}
