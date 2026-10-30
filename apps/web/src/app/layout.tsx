import type { Metadata } from "next";
import "./globals.css";
import {cookies,headers} from 'next/headers';
import {LocaleProvider} from '@/components/locale-provider';
import {normalizeLocale,preferredLocale} from '@/lib/locale';

export const metadata: Metadata = {
  title: "AI Phone",
  description: "AI Phone",
  robots: { index: false, follow: false },
};

export default async function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const saved=(await cookies()).get('ai-phone-locale')?.value;
  const configured=saved||process.env.AI_PHONE_DEFAULT_LOCALE;
  const locale=configured?normalizeLocale(configured):preferredLocale((await headers()).get('accept-language'));
  return <html dir={locale==='ar'?'rtl':'ltr'} lang={locale}><body><LocaleProvider initialLocale={locale}>{children}</LocaleProvider></body></html>;
}

