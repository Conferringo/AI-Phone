"use client";
import {Button} from "@/components/ui/button";
import {useI18n} from "@/components/locale-provider";
export default function PhoneError({reset}:{reset:()=>void}){const {t}=useI18n();return <main className="entry-screen"><div className="entry-card"><p role="alert">{t("连接失败")}</p><Button onClick={reset}>{t("重试")}</Button></div></main>;}
