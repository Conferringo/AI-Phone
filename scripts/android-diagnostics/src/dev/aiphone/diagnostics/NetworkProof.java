package dev.aiphone.diagnostics;
import android.app.*;import android.os.Bundle;import android.widget.TextView;import org.json.JSONObject;import javax.net.ssl.HttpsURLConnection;import java.net.URL;import java.io.*;import java.nio.charset.StandardCharsets;
/** Temporary real-network/notification test; no launcher icon or user account access. */
public final class NetworkProof extends Activity {
 private String nonce; private TextView text;
 private void record(String status){try{JSONObject value=new JSONObject();value.put("status",status);value.put("nonce",nonce);value.put("at",System.currentTimeMillis());try(FileOutputStream file=openFileOutput("network-proof.json",MODE_PRIVATE)){file.write(value.toString().getBytes(StandardCharsets.UTF_8));}}catch(Exception ignored){}runOnUiThread(()->text.setText("Network acceptance: "+status));}
 @Override public void onCreate(Bundle state){super.onCreate(state);nonce=getIntent().getStringExtra("nonce");if(nonce==null||!nonce.matches("[a-f0-9]{32}")){finish();return;}text=new TextView(this);text.setTextSize(20);text.setPadding(32,48,32,32);setContentView(text);record("connecting");new Thread(this::listen).start();}
 private void listen(){HttpsURLConnection connection=null;try{
  connection=(HttpsURLConnection)new URL("https://ntfy.sh/aiphone-probe-"+nonce+"/json").openConnection();connection.setConnectTimeout(15000);connection.setReadTimeout(45000);connection.setRequestProperty("User-Agent","AI-Phone-network-acceptance");
  if(connection.getResponseCode()!=200)throw new IOException();
  try(BufferedReader reader=new BufferedReader(new InputStreamReader(connection.getInputStream(),StandardCharsets.UTF_8))){String line;long deadline=System.currentTimeMillis()+60000;while(System.currentTimeMillis()<deadline&&(line=reader.readLine())!=null){JSONObject event=new JSONObject(line);if("open".equals(event.optString("event")))record("listening");if("message".equals(event.optString("event"))&&nonce.equals(event.optString("message"))){NotificationManager manager=getSystemService(NotificationManager.class);manager.createNotificationChannel(new NotificationChannel("network-proof","网络验收",NotificationManager.IMPORTANCE_DEFAULT));manager.notify(19,new Notification.Builder(this,"network-proof").setSmallIcon(android.R.drawable.stat_notify_sync).setContentTitle("AI Phone 网络验收").setContentText("外部网络测试通知 "+nonce).setAutoCancel(true).build());record("notified");return;}}}
  record("timeout");
 }catch(Exception failure){record("failed");}finally{if(connection!=null)connection.disconnect();}}
}
