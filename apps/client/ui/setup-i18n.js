const keys=['title','phone','desktop','cpu','memory','storage','install','existing','back','ready','retry','cancel','accept','enter','memoryCheck','storageCheck','hostPending','hostResources','failed','cancelled','commandLineTools','invalidPayload','downloadFailed','busy','checking','runtime','source','toolchain','sdk-tools','container-host','sdk-licenses','sdk-packages','dependencies','native-apps','workspace-image','desktop-image','services','devices'];
const rows={
 'zh-CN':['配置 AI Phone','手机','电脑','处理器','内存','存储','在本机安装','使用已有部署','返回','已就绪','重试','停止','同意并继续','配置模型','内存','可用空间','此平台的独立宿主仍在开发中。','本机资源不足，请检查内存和可用空间。','安装未完成。检查网络或环境后重试；日志保存在本机安装目录。','已停止。已完成的步骤和用户数据会保留。','请完成 macOS 弹出的开发工具安装，再点重试。','安装包校验失败，请重新下载客户端。','依赖下载失败，请检查网络后重试。','正在安装…','检查本机','准备运行环境','校验源码','安装开发工具','准备 Android SDK','准备容器环境','Android 许可证','安装 Android 系统','安装服务依赖','准备设备应用','构建工作环境','构建 Linux 桌面','启动服务','验证设备'],
 en:['Set up AI Phone','Phone','Computer','Processors','Memory','Storage','Install on this device','Use an existing deployment','Back','Ready','Retry','Stop','Accept and continue','Configure model','Memory','Free space','The standalone host for this platform is still in development.','Insufficient resources. Check memory and free space.','Installation incomplete. Check the network or environment, then retry. Logs are in the local installation directory.','Stopped. Completed steps and user data are retained.','Complete the macOS developer tools installation, then retry.','Payload verification failed. Download the client again.','Dependency download failed. Check your network and retry.','Installing…','Checking this device','Preparing runtime','Verifying source','Installing development tools','Preparing Android SDK','Preparing container host','Android licenses','Installing Android','Installing service dependencies','Preparing device apps','Building workspace','Building Linux desktop','Starting services','Verifying devices'],
 'zh-TW':['設定 AI Phone','手機','電腦','處理器','記憶體','儲存','在本機安裝','使用現有部署','返回','已就緒','重試','停止','同意並繼續','設定模型','記憶體','可用空間','此平台的獨立宿主仍在開發中。','本機資源不足，請檢查記憶體和可用空間。','安裝未完成。檢查網路或環境後重試；日誌保存在本機安裝目錄。','已停止，已完成步驟和使用者資料會保留。','請完成 macOS 的開發工具安裝，再重試。','安裝包驗證失敗，請重新下載客戶端。','依賴下載失敗，請檢查網路後重試。','正在安裝…','檢查本機','準備執行環境','驗證原始碼','安裝開發工具','準備 Android SDK','準備容器環境','Android 授權條款','安裝 Android','安裝服務依賴','準備裝置應用','建置工作環境','建置 Linux 桌面','啟動服務','驗證裝置'],
 ja:['AI Phone を設定','スマートフォン','コンピューター','CPU','メモリ','ストレージ','このデバイスにインストール','既存の環境を使う','戻る','準備完了','再試行','停止','同意して続行','モデルを設定','メモリ','空き容量','このプラットフォームの独立ホストは開発中です。','メモリと空き容量を確認してください。','インストール未完了。環境を確認して再試行してください。ログはローカルフォルダーにあります。','停止しました。データは保持されます。','macOS の開発ツールをインストールして再試行してください。','パッケージ検証失敗。再ダウンロードしてください。','ダウンロード失敗。ネットワークを確認してください。','インストール中…','デバイスを確認','実行環境を準備','ソースを検証','開発ツールをインストール','Android SDK を準備','コンテナ環境を準備','Android ライセンス','Android をインストール','依存関係をインストール','アプリを準備','作業環境を構築','Linux デスクトップを構築','サービスを起動','デバイスを検証'],
 ko:['AI Phone 설정','휴대폰','컴퓨터','프로세서','메모리','저장 공간','이 기기에 설치','기존 배포 사용','뒤로','준비 완료','재시도','중지','동의하고 계속','모델 설정','메모리','여유 공간','이 플랫폼의 독립 호스트는 개발 중입니다.','메모리와 여유 공간을 확인하세요.','설치 미완료. 환경을 확인하고 재시도하세요. 로그는 로컬 설치 폴더에 있습니다.','중지됨. 데이터는 유지됩니다.','macOS 개발 도구 설치 후 재시도하세요.','패키지 검증 실패. 다시 다운로드하세요.','다운로드 실패. 네트워크를 확인하세요.','설치 중…','기기 확인','런타임 준비','소스 검증','개발 도구 설치','Android SDK 준비','컨테이너 환경 준비','Android 라이선스','Android 설치','서비스 종속성 설치','기기 앱 준비','작업 환경 빌드','Linux 데스크톱 빌드','서비스 시작','기기 검증'],
 de:['AI Phone einrichten','Telefon','Computer','Prozessoren','Arbeitsspeicher','Speicher','Auf diesem Gerät installieren','Vorhandene Installation verwenden','Zurück','Bereit','Erneut versuchen','Stoppen','Zustimmen und fortfahren','Modell einrichten','Arbeitsspeicher','Freier Speicher','Der eigenständige Host für diese Plattform ist noch in Entwicklung.','Arbeitsspeicher und freien Speicher prüfen.','Installation unvollständig. Umgebung prüfen und erneut versuchen. Protokolle liegen im lokalen Installationsordner.','Gestoppt. Benutzerdaten bleiben erhalten.','macOS-Entwicklertools installieren und erneut versuchen.','Paketprüfung fehlgeschlagen. Client erneut herunterladen.','Download fehlgeschlagen. Netzwerk prüfen.','Installation läuft…','Gerät prüfen','Laufzeit vorbereiten','Quellcode prüfen','Entwicklertools installieren','Android SDK vorbereiten','Container-Host vorbereiten','Android-Lizenzen','Android installieren','Abhängigkeiten installieren','Geräte-Apps vorbereiten','Arbeitsumgebung erstellen','Linux-Desktop erstellen','Dienste starten','Geräte prüfen'],
 fr:['Configurer AI Phone','Téléphone','Ordinateur','Processeurs','Mémoire','Stockage','Installer sur cet appareil','Utiliser un déploiement existant','Retour','Prêt','Réessayer','Arrêter','Accepter et continuer','Configurer le modèle','Mémoire','Espace libre','L’hôte autonome pour cette plateforme est encore en développement.','Vérifiez la mémoire et l’espace libre.','Installation incomplète. Vérifiez l’environnement, puis réessayez. Les journaux sont dans le dossier local d’installation.','Arrêté. Les données sont conservées.','Installez les outils de développement macOS, puis réessayez.','Vérification échouée. Téléchargez à nouveau le client.','Échec du téléchargement. Vérifiez le réseau.','Installation…','Vérification de l’appareil','Préparation de l’environnement','Vérification du code source','Installation des outils','Préparation du SDK Android','Préparation des conteneurs','Licences Android','Installation d’Android','Installation des dépendances','Préparation des applications','Création de l’environnement de travail','Création du bureau Linux','Démarrage des services','Vérification des appareils'],
 es:['Configurar AI Phone','Teléfono','Ordenador','Procesadores','Memoria','Almacenamiento','Instalar en este dispositivo','Usar un despliegue existente','Volver','Listo','Reintentar','Detener','Aceptar y continuar','Configurar modelo','Memoria','Espacio libre','El servidor independiente de esta plataforma sigue en desarrollo.','Revisa la memoria y el espacio libre.','Instalación incompleta. Revisa el entorno y reinténtalo. Los registros están en la carpeta local de instalación.','Detenido. Los datos se conservan.','Instala las herramientas de desarrollo de macOS y reinténtalo.','Error de verificación. Descarga el cliente de nuevo.','Error de descarga. Revisa la red.','Instalando…','Comprobando dispositivo','Preparando entorno','Verificando código','Instalando herramientas','Preparando Android SDK','Preparando contenedores','Licencias de Android','Instalando Android','Instalando dependencias','Preparando aplicaciones','Creando entorno de trabajo','Creando escritorio Linux','Iniciando servicios','Verificando dispositivos']
};
export const setupLabels=Object.fromEntries(Object.entries(rows).map(([locale,values])=>[locale,Object.fromEntries(keys.map((key,index)=>[key,values[index]]))]));

const recovery={
 en:['Installed','Setup interrupted','Start','Saved setup is unreadable. Retry setup.','Cannot save setup progress. Check free space and folder permissions.'],
 'zh-CN':['已安装','安装已中断','启动','安装记录无法读取，请重试。','无法保存安装进度，请检查空间和目录权限。'],
 'zh-TW':['已安裝','安裝已中斷','啟動','無法讀取安裝記錄，請重試。','無法儲存安裝進度，請檢查空間與目錄權限。'],
 ja:['インストール済み','設定が中断されました','起動','設定記録を読めません。再試行してください。','進行状況を保存できません。空き容量と権限を確認してください。'],
 ko:['설치됨','설정 중단됨','시작','설정 기록을 읽을 수 없습니다. 다시 시도하세요.','진행 상황을 저장할 수 없습니다. 공간과 권한을 확인하세요.'],
 de:['Installiert','Einrichtung unterbrochen','Starten','Einrichtungsdaten nicht lesbar. Erneut versuchen.','Fortschritt nicht gespeichert. Speicherplatz und Ordnerrechte prüfen.'],
 fr:['Installé','Installation interrompue','Démarrer','Données illisibles. Réessayez la configuration.','Progression non enregistrée. Vérifiez l’espace libre et les droits du dossier.'],
 es:['Instalado','Configuración interrumpida','Iniciar','No se puede leer la configuración. Reintenta.','No se puede guardar el progreso. Revisa el espacio y los permisos.']
};
for(const [locale,values] of Object.entries(recovery))Object.assign(setupLabels[locale],Object.fromEntries(['installed','interrupted','start','invalidCheckpoint','checkpointFailed'].map((key,index)=>[key,values[index]])));

const phaseNames={en:['Installation incomplete','Stopped','Developer tools'],'zh-CN':['安装未完成','已停止','开发工具'],'zh-TW':['安裝未完成','已停止','開發工具'],ja:['インストール未完了','停止','開発ツール'],ko:['설치 미완료','중지됨','개발 도구'],de:['Installation unvollständig','Gestoppt','Entwicklertools'],fr:['Installation incomplète','Arrêtée','Outils de développement'],es:['Instalación incompleta','Detenida','Herramientas de desarrollo']};
for(const [locale,values] of Object.entries(phaseNames))Object.assign(setupLabels[locale],Object.fromEntries(['installFailed','stopped','system-tools'].map((key,index)=>[key,values[index]])));

const ownership={en:'An existing installation owns the services. Choose “Use an existing deployment” to keep using it.','zh-CN':'本机已有部署正在使用这些服务，请选择“使用已有部署”。','zh-TW':'本機已有部署正在使用這些服務，請選擇「使用現有部署」。',ja:'既存の環境がサービスを使用中です。「既存の環境を使う」を選んでください。',ko:'기존 설치가 서비스를 사용 중입니다. “기존 배포 사용”을 선택하세요.',de:'Eine vorhandene Installation nutzt die Dienste. „Vorhandene Installation verwenden“ wählen.',fr:'Une installation existante utilise les services. Choisissez « Utiliser un déploiement existant ».',es:'Una instalación existente usa los servicios. Elige « Usar un despliegue existente ».'};
for(const [locale,text] of Object.entries(ownership))setupLabels[locale].foreignInstallation=text;

const linuxErrors={
 en:['Local installation currently supports Ubuntu and Debian x64.','System authorization is needed to install dependencies and access KVM. Use a desktop session with PolicyKit.','KVM is unavailable. Enable hardware virtualization in firmware and check /dev/kvm. No devices were changed.'],
 'zh-CN':['本机安装目前支持 Ubuntu / Debian x64。','安装依赖和访问 KVM 需要系统授权，请在具有 PolicyKit 的桌面会话中重试。','KVM 不可用，请开启固件虚拟化并检查 /dev/kvm。设备数据未改变。'],
 'zh-TW':['本機安裝目前支援 Ubuntu / Debian x64。','安裝相依套件與存取 KVM 需要系統授權，請在具有 PolicyKit 的桌面工作階段重試。','KVM 無法使用，請啟用韌體虛擬化並檢查 /dev/kvm。裝置資料未變更。'],
 ja:['ローカルインストールは現在 Ubuntu / Debian x64 に対応しています。','依存関係と KVM へのアクセスにはシステム認証が必要です。PolicyKit のあるデスクトップで再試行してください。','KVM を使用できません。ファームウェアの仮想化と /dev/kvm を確認してください。デバイスデータは保持されています。'],
 ko:['현재 로컬 설치는 Ubuntu / Debian x64를 지원합니다.','종속성 설치와 KVM 접근에 시스템 승인이 필요합니다. PolicyKit 데스크톱에서 다시 시도하세요.','KVM을 사용할 수 없습니다. 펌웨어 가상화와 /dev/kvm을 확인하세요. 기기 데이터는 유지됩니다.'],
 de:['Lokale Installation unterstützt derzeit Ubuntu / Debian x64.','Abhängigkeiten und KVM benötigen Systemfreigabe. In einer Desktop-Sitzung mit PolicyKit erneut versuchen.','KVM nicht verfügbar. Firmware-Virtualisierung und /dev/kvm prüfen. Gerätedaten wurden nicht verändert.'],
 fr:['L’installation locale prend actuellement en charge Ubuntu / Debian x64.','Les dépendances et KVM nécessitent une autorisation système. Réessayez dans une session de bureau avec PolicyKit.','KVM indisponible. Vérifiez la virtualisation du firmware et /dev/kvm. Les données sont conservées.'],
 es:['La instalación local admite Ubuntu / Debian x64.','Las dependencias y KVM requieren autorización del sistema. Reintenta en una sesión con PolicyKit.','KVM no disponible. Revisa la virtualización del firmware y /dev/kvm. Los datos no se han modificado.']
};
for(const [locale,values] of Object.entries(linuxErrors))Object.assign(setupLabels[locale],Object.fromEntries(['linuxDistribution','linuxAuthorization','virtualizationUnavailable'].map((key,index)=>[key,values[index]])));

setupLabels.ru={
  "title": "Настроить AI Phone",
  "phone": "Телефон",
  "desktop": "Компьютер",
  "cpu": "Процессоры",
  "memory": "Память",
  "storage": "Хранилище",
  "install": "Установить на этом устройстве",
  "existing": "Использовать существующую установку",
  "back": "Назад",
  "ready": "Готово",
  "retry": "Повторить",
  "cancel": "Остановить",
  "accept": "Принять и продолжить",
  "enter": "Настроить модель",
  "memoryCheck": "Память",
  "storageCheck": "Свободное место",
  "hostPending": "Автономный хост для этой платформы ещё в разработке.",
  "hostResources": "Недостаточно ресурсов. Проверьте память и свободное место.",
  "failed": "Установка не завершена. Проверьте сеть или среду и повторите. Журналы находятся в локальной папке установки.",
  "cancelled": "Остановлено. Завершённые шаги и пользовательские данные сохранены.",
  "commandLineTools": "Завершите установку инструментов разработчика macOS и повторите.",
  "invalidPayload": "Проверка пакета не пройдена. Скачайте клиент заново.",
  "downloadFailed": "Не удалось скачать зависимости. Проверьте сеть и повторите.",
  "busy": "Установка…",
  "checking": "Проверка устройства",
  "runtime": "Подготовка среды",
  "source": "Проверка исходного кода",
  "toolchain": "Установка инструментов разработчика",
  "sdk-tools": "Подготовка Android SDK",
  "container-host": "Подготовка контейнерной среды",
  "sdk-licenses": "Лицензии Android",
  "sdk-packages": "Установка Android",
  "dependencies": "Установка зависимостей сервисов",
  "native-apps": "Подготовка приложений устройств",
  "workspace-image": "Сборка рабочей среды",
  "desktop-image": "Сборка рабочего стола Linux",
  "services": "Запуск сервисов",
  "devices": "Проверка устройств",
  "installed": "Установлено",
  "interrupted": "Настройка прервана",
  "start": "Запустить",
  "invalidCheckpoint": "Сохранённые настройки не читаются. Повторите настройку.",
  "checkpointFailed": "Не удалось сохранить прогресс. Проверьте место на диске и права на папку.",
  "installFailed": "Установка не завершена",
  "stopped": "Остановлено",
  "system-tools": "Инструменты разработчика",
  "foreignInstallation": "Существующая установка использует сервисы. Выберите «Использовать существующую установку».",
  "linuxDistribution": "Локальная установка пока поддерживает Ubuntu и Debian x64.",
  "linuxAuthorization": "Для установки зависимостей и доступа к KVM требуется системная авторизация. Используйте рабочий стол с PolicyKit.",
  "virtualizationUnavailable": "KVM недоступен. Включите виртуализацию в прошивке и проверьте /dev/kvm. Данные устройств не изменены."
};

setupLabels.ar={
  "title": "إعداد AI Phone",
  "phone": "الهاتف",
  "desktop": "الحاسوب",
  "cpu": "المعالجات",
  "memory": "الذاكرة",
  "storage": "التخزين",
  "install": "التثبيت على هذا الجهاز",
  "existing": "استخدام نشر موجود",
  "back": "رجوع",
  "ready": "جاهز",
  "retry": "إعادة المحاولة",
  "cancel": "إيقاف",
  "accept": "موافقة ومتابعة",
  "enter": "إعداد النموذج",
  "memoryCheck": "الذاكرة",
  "storageCheck": "المساحة المتاحة",
  "hostPending": "المضيف المستقل لهذه المنصة لا يزال قيد التطوير.",
  "hostResources": "الموارد غير كافية. تحقق من الذاكرة والمساحة المتاحة.",
  "failed": "لم يكتمل التثبيت. افحص الشبكة أو البيئة ثم أعد المحاولة. السجلات في مجلد التثبيت المحلي.",
  "cancelled": "تم الإيقاف. الخطوات المكتملة وبيانات المستخدم محفوظة.",
  "commandLineTools": "أكمل تثبيت أدوات تطوير macOS ثم أعد المحاولة.",
  "invalidPayload": "فشل التحقق من الحزمة. أعد تنزيل العميل.",
  "downloadFailed": "فشل تنزيل التبعيات. افحص الشبكة وأعد المحاولة.",
  "busy": "جارٍ التثبيت…",
  "checking": "جارٍ فحص الجهاز",
  "runtime": "جارٍ إعداد بيئة التشغيل",
  "source": "جارٍ التحقق من المصدر",
  "toolchain": "جارٍ تثبيت أدوات التطوير",
  "sdk-tools": "جارٍ إعداد Android SDK",
  "container-host": "جارٍ إعداد مضيف الحاويات",
  "sdk-licenses": "تراخيص Android",
  "sdk-packages": "جارٍ تثبيت Android",
  "dependencies": "جارٍ تثبيت تبعيات الخدمات",
  "native-apps": "جارٍ إعداد تطبيقات الأجهزة",
  "workspace-image": "جارٍ بناء مساحة العمل",
  "desktop-image": "جارٍ بناء سطح مكتب Linux",
  "services": "جارٍ تشغيل الخدمات",
  "devices": "جارٍ التحقق من الأجهزة",
  "installed": "مثبت",
  "interrupted": "انقطع الإعداد",
  "start": "بدء",
  "invalidCheckpoint": "تعذرت قراءة الإعداد المحفوظ. أعد الإعداد.",
  "checkpointFailed": "تعذر حفظ التقدم. تحقق من المساحة وأذونات المجلد.",
  "installFailed": "التثبيت غير مكتمل",
  "stopped": "متوقف",
  "system-tools": "أدوات التطوير",
  "foreignInstallation": "هناك نشر موجود يستخدم الخدمات. اختر «استخدام نشر موجود» للاحتفاظ به.",
  "linuxDistribution": "يدعم التثبيت المحلي حاليًا Ubuntu وDebian x64.",
  "linuxAuthorization": "يلزم تفويض النظام لتثبيت التبعيات والوصول إلى KVM. استخدم جلسة سطح مكتب مع PolicyKit.",
  "virtualizationUnavailable": "KVM غير متاح. فعّل المحاكاة الافتراضية في البرنامج الثابت وافحص /dev/kvm. لم تتغير بيانات الأجهزة."
};
