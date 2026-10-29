import {NextRequest,NextResponse} from 'next/server';
import {entryLanguage} from '@/lib/locale';

// Navigation preference only. Business API and authentication still run downstream.
export function proxy(request:NextRequest){
 const selected=request.nextUrl.searchParams.get('language');
 if(selected===null)return NextResponse.next();
 const url=request.nextUrl.clone();url.searchParams.delete('language');
 const response=NextResponse.redirect(url,307),locale=entryLanguage(selected);
 if(locale)response.cookies.set('ai-phone-locale',locale,{path:'/',maxAge:31536000,sameSite:'lax'});
 return response;
}
export const config={matcher:['/phone']};
