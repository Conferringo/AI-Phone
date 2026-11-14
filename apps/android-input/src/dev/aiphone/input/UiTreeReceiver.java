package dev.aiphone.input;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.util.Base64;
import android.view.accessibility.AccessibilityNodeInfo;
import java.nio.charset.StandardCharsets;

/** DUMP-protected, read-only request. No file, cached tree, or input action. */
public final class UiTreeReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context, Intent intent) {
        setResultCode(Activity.RESULT_CANCELED);
        String request=intent.getStringExtra("request_id");
        if(!"dev.aiphone.input.DUMP_UI".equals(intent.getAction())||request==null||!request.matches("[a-f0-9]{32}"))return;
        AccessibilityNodeInfo root=null;
        try {
            root=PhoneAttention.activeRoot();
            if(root==null)return;
            byte[] xml=UiHierarchy.capture(root).getBytes(StandardCharsets.UTF_8);
            if(xml.length>262144)return;
            setResultData(request+":"+Base64.encodeToString(xml,Base64.NO_WRAP));
            setResultCode(Activity.RESULT_OK);
        } catch(Exception unavailable) {
            // An incomplete or unavailable hierarchy must fall back to another source.
        } finally { if(root!=null)root.recycle(); }
    }
}
