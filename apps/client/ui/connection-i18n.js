export const connectionLanguages=['zh-CN','en','zh-TW','ja','ko','de','fr','es'];
export const connectionLanguageNames=['简体中文','English','繁體中文','日本語','한국어','Deutsch','Français','Español'];
export function connectionLocale(value=''){
 const tag=value.toLowerCase();
 if(/^zh-(tw|hk|hant)/.test(tag))return 'zh-TW';
 if(tag.startsWith('zh'))return 'zh-CN';
 const base=tag.split('-')[0];return connectionLanguages.includes(base)?base:'en';
}
export const connectionLabels={
 'zh-CN':['连接 AI Phone','服务器','连接','正在连接…','无法连接，请先启动部署服务。','服务器地址无效','请填写不含账号、密码或路径的 HTTP(S) 地址','请选择本机或局域网地址','正在连接…'],
 en:['Connect to AI Phone','Server','Connect','Connecting…','Cannot connect. Start your deployment, then retry.','Invalid server address','Use an HTTP(S) address without credentials or a path','Choose localhost or a private LAN address','Connection already in progress'],
 'zh-TW':['連接 AI Phone','伺服器','連接','正在連接…','無法連接，請先啟動部署服務。','伺服器網址無效','請填寫不含帳號、密碼或路徑的 HTTP(S) 網址','請選擇本機或區域網路位址','正在連接…'],
 ja:['AI Phone に接続','サーバー','接続','接続中…','接続できません。サーバーを起動して再試行してください。','無効なサーバーアドレス','認証情報やパスを含まない HTTP(S) アドレスを入力してください','localhost またはプライベート LAN のアドレスを選択してください','接続処理中です'],
 ko:['AI Phone 연결','서버','연결','연결 중…','연결할 수 없습니다. 서버를 시작한 후 다시 시도하세요.','잘못된 서버 주소','인증 정보나 경로가 없는 HTTP(S) 주소를 입력하세요','localhost 또는 사설 LAN 주소를 선택하세요','이미 연결 중입니다'],
 de:['Mit AI Phone verbinden','Server','Verbinden','Verbindung wird hergestellt…','Keine Verbindung. Starten Sie den Server und versuchen Sie es erneut.','Ungültige Serveradresse','HTTP(S)-Adresse ohne Zugangsdaten oder Pfad eingeben','Localhost oder eine private LAN-Adresse wählen','Verbindung wird bereits hergestellt'],
 fr:['Connexion à AI Phone','Serveur','Connecter','Connexion…','Connexion impossible. Démarrez le serveur et réessayez.','Adresse du serveur invalide','Saisissez une adresse HTTP(S) sans identifiants ni chemin','Choisissez localhost ou une adresse LAN privée','Connexion déjà en cours'],
 es:['Conectar con AI Phone','Servidor','Conectar','Conectando…','No se puede conectar. Inicia el servidor e inténtalo de nuevo.','Dirección del servidor no válida','Introduce una dirección HTTP(S) sin credenciales ni ruta','Elige localhost o una dirección LAN privada','La conexión ya está en curso']
};
