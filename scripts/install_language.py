"""Language choice and labels for the installation configuration boundary."""
LOCALES=('zh-CN','en','zh-TW','ja','ko','de','fr','es','ru','ar')
NAMES=('简体中文','English','繁體中文','日本語','한국어','Deutsch','Français','Español','Русский','العربية')
_KEYS=('heading','host_proxy','container_proxy','phone','desktop','cpus','memory_mb','storage_gb','saved','invalid')
_LABELS=(
 ('本机配置 · 保留已有系统盘','本机代理（留空保持，输入 - 直联）','容器代理（留空保持，输入 - 直联）','手机','电脑','处理器','内存 MB','存储 GB','配置已保存。已有设备资源未改动，调整需单独迁移。','请选择列表中的语言。'),
 ('Host configuration · existing disks preserved','Host proxy (empty: keep; -: direct)','Container proxy (empty: keep; -: direct)','Phone','Computer','CPU cores','Memory MB','Storage GB','Configuration saved. Existing device resources were preserved; changes need a separate migration.','Choose a language from the list.'),
 ('本機配置 · 保留已有系統碟','本機代理（留空保持，輸入 - 直連）','容器代理（留空保持，輸入 - 直連）','手機','電腦','處理器','記憶體 MB','儲存 GB','配置已儲存。已有裝置資源未變動，調整需另行遷移。','請選擇清單中的語言。'),
 ('ホスト設定 · 既存ディスクを保持','ホストプロキシ（空欄: 維持、-: 直接接続）','コンテナプロキシ（空欄: 維持、-: 直接接続）','スマートフォン','コンピューター','CPU コア','メモリ MB','ストレージ GB','設定を保存しました。既存デバイスのリソース変更には別途移行が必要です。','一覧から言語を選択してください。'),
 ('호스트 설정 · 기존 디스크 보존','호스트 프록시 (빈칸: 유지, -: 직접 연결)','컨테이너 프록시 (빈칸: 유지, -: 직접 연결)','휴대폰','컴퓨터','CPU 코어','메모리 MB','저장 공간 GB','설정을 저장했습니다. 기존 기기 리소스 변경은 별도 마이그레이션이 필요합니다.','목록에서 언어를 선택하세요.'),
 ('Hostkonfiguration · vorhandene Datenträger bleiben erhalten','Host-Proxy (leer: behalten; -: direkt)','Container-Proxy (leer: behalten; -: direkt)','Telefon','Computer','CPU-Kerne','Arbeitsspeicher MB','Speicher GB','Konfiguration gespeichert. Vorhandene Geräteressourcen bleiben erhalten; Änderungen erfordern eine separate Migration.','Wählen Sie eine Sprache aus der Liste.'),
 ('Configuration · disques existants conservés','Proxy hôte (vide : conserver ; - : direct)','Proxy conteneur (vide : conserver ; - : direct)','Téléphone','Ordinateur','Cœurs CPU','Mémoire MB','Stockage GB','Configuration enregistrée. Les ressources existantes sont conservées ; leur modification nécessite une migration distincte.','Choisissez une langue dans la liste.'),
 ('Configuración · discos existentes conservados','Proxy del host (vacío: conservar; -: directo)','Proxy del contenedor (vacío: conservar; -: directo)','Teléfono','Ordenador','Núcleos CPU','Memoria MB','Almacenamiento GB','Configuración guardada. Los recursos existentes se conservan; los cambios requieren una migración independiente.','Elige un idioma de la lista.'),
 ('Настройки хоста · существующие диски сохранены','Прокси хоста (пусто: сохранить; -: напрямую)','Прокси контейнера (пусто: сохранить; -: напрямую)','Телефон','Компьютер','Ядра CPU','Память МБ','Хранилище ГБ','Настройки сохранены. Ресурсы существующих устройств не изменены; нужна отдельная миграция.','Выберите язык из списка.'),
 ('إعداد المضيف · الأقراص الحالية محفوظة','وكيل المضيف (فارغ: إبقاء؛ -: مباشر)','وكيل الحاويات (فارغ: إبقاء؛ -: مباشر)','الهاتف','الحاسوب','أنوية المعالج','الذاكرة MB','التخزين GB','حُفظ الإعداد. موارد الأجهزة الحالية محفوظة؛ يتطلب تعديلها ترحيلًا منفصلًا.','اختر لغة من القائمة.')
)
CATALOGS={locale:dict(zip(_KEYS,labels,strict=True)) for locale,labels in zip(LOCALES,_LABELS,strict=True)}


def choose_language(current):
    choices=' / '.join(f'{i+1} {name}' for i,name in enumerate(NAMES))
    while True:
        answer=input(f'Language / 语言 — {choices} [{current}]: ').strip()
        if not answer:return current
        if answer in LOCALES:return answer
        if answer.isascii() and answer.isdigit() and 1<=int(answer)<=len(LOCALES):return LOCALES[int(answer)-1]
        print(CATALOGS[current]['invalid'])
