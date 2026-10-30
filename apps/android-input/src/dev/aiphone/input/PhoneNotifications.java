package dev.aiphone.input;

import android.app.Notification;
import android.os.Handler;
import android.os.Looper;
import android.service.notification.NotificationListenerService;
import android.service.notification.StatusBarNotification;
import android.util.AtomicFile;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayDeque;
import java.util.HashSet;

/** Service-only notification feed. No launcher icon and no automatic replies or dismissals. */
public final class PhoneNotifications extends NotificationListenerService {
    private final ArrayDeque<JSONObject> events = new ArrayDeque<>();
    private final HashSet<String> seen = new HashSet<>();
    private final Handler main = new Handler(Looper.getMainLooper());
    private AtomicFile file;
    private boolean connected;
    private int bufferedBytes;
    private final Runnable heartbeat = new Runnable() { public void run() { persist(); main.postDelayed(this, 30000); } };
    @Override public void onCreate() {
        super.onCreate();
        file = new AtomicFile(new File(getFilesDir(), "notifications.json"));
        try {
            JSONObject saved = new JSONObject(new String(file.readFully(), StandardCharsets.UTF_8));
            JSONArray old = saved.optJSONArray("notifications");
            if(old != null) for(int i=Math.max(0,old.length()-160);i<old.length();i++) {
                JSONObject event=old.getJSONObject(i); append(event);
            }
        } catch(Exception ignored) {}
        main.post(heartbeat);
    }
    @Override public void onListenerConnected() {
        connected=true;
        try { for(StatusBarNotification notification:getActiveNotifications()) remember(notification); } catch(Exception ignored) {}
        persist();
    }
    @Override public void onListenerDisconnected() { connected=false; persist(); }
    @Override public void onNotificationPosted(StatusBarNotification notification) { remember(notification); persist(); }
    @Override public void onNotificationRemoved(StatusBarNotification notification) { persist(); }
    @Override public void onDestroy() { connected=false;main.removeCallbacksAndMessages(null);persist();super.onDestroy(); }
    private static String bounded(Object text,int max) { String s=text==null?"":text.toString();return s.length()>max?s.substring(0,max):s; }
    private void remember(StatusBarNotification sbn) {
        try {
            Notification notification=sbn.getNotification();
            if((notification.flags&(Notification.FLAG_ONGOING_EVENT|Notification.FLAG_GROUP_SUMMARY))!=0) return;
            String title=bounded(notification.extras.getCharSequence(Notification.EXTRA_TITLE),500);
            String body=bounded(notification.extras.getCharSequence(Notification.EXTRA_BIG_TEXT),4000);
            if(body.isEmpty())body=bounded(notification.extras.getCharSequence(Notification.EXTRA_TEXT),4000);
            if(title.isEmpty()&&body.isEmpty())return;
            // Android reposts the same data-usage warning repeatedly. Social notifications
            // retain their timestamp so genuinely repeated incoming messages still wake us.
            boolean system = "android".equals(sbn.getPackageName()) || "com.android.systemui".equals(sbn.getPackageName());
            String identity=sbn.getKey()+"\n"+(system?"system":sbn.getPostTime())+"\n"+title+"\n"+body;
            byte[] digest=MessageDigest.getInstance("SHA-256").digest(identity.getBytes(StandardCharsets.UTF_8));
            StringBuilder hash=new StringBuilder();for(byte b:digest)hash.append(String.format("%02x",b&255));
            String id=hash.toString();if(seen.contains(id))return;
            String app=sbn.getPackageName();try {app=getPackageManager().getApplicationLabel(getPackageManager().getApplicationInfo(app,0)).toString();}catch(Exception ignored){}
            JSONObject value=new JSONObject();value.put("event_id",id);value.put("notification_key",bounded(sbn.getKey(),1024));value.put("package_name",sbn.getPackageName());value.put("app_name",bounded(app,200));value.put("title",title);value.put("text",body);value.put("posted_at_ms",sbn.getPostTime());append(value);
        } catch(Exception ignored) {}
    }
    private void append(JSONObject value) {
        events.add(value);seen.add(value.optString("event_id"));
        bufferedBytes+=value.toString().getBytes(StandardCharsets.UTF_8).length;
        // Bound both count and bytes: long CJK messages must not exceed the ADB transport limit.
        while(events.size()>160||bufferedBytes>1200000) {
            JSONObject removed=events.removeFirst();seen.remove(removed.optString("event_id"));
            bufferedBytes-=removed.toString().getBytes(StandardCharsets.UTF_8).length;
        }
    }
    private void persist() {
        if(file==null)return;
        FileOutputStream stream=null;
        try {
            JSONObject value=new JSONObject();value.put("connected",connected);value.put("checked_at_ms",System.currentTimeMillis());value.put("notifications",new JSONArray(events));
            stream=file.startWrite();stream.write(value.toString().getBytes(StandardCharsets.UTF_8));file.finishWrite(stream);
        } catch(Exception ignored) { if(stream!=null)file.failWrite(stream); }
    }
}
