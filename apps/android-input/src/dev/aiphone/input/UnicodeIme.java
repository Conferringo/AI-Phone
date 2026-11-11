package dev.aiphone.input;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.inputmethodservice.InputMethodService;
import android.util.Base64;
import android.view.View;
import android.view.inputmethod.InputConnection;
import android.widget.TextView;
import java.nio.charset.StandardCharsets;

/** Project-owned input bridge. Only DUMP-authorized shell/root senders may use it. */
public final class UnicodeIme extends InputMethodService {
    private final BroadcastReceiver input = new BroadcastReceiver() {
        @Override public void onReceive(Context context, Intent intent) {
            InputConnection connection = getCurrentInputConnection();
            if (connection == null) { setResultCode(2); setResultData("no_active_editor"); return; }
            try {
                String payload = intent.getStringExtra("text_b64");
                if (payload == null || payload.length() > 24000) throw new IllegalArgumentException();
                String text = new String(Base64.decode(payload, Base64.DEFAULT), StandardCharsets.UTF_8);
                if (!connection.commitText(text, 1)) { setResultCode(2); setResultData("editor_rejected"); return; }
                setResultCode(1);
                setResultData("committed:" + intent.getStringExtra("action_id"));
            } catch (Exception error) { setResultCode(2); setResultData("invalid_input"); }
        }
    };

    @Override public void onCreate() {
        super.onCreate();
        registerReceiver(input, new IntentFilter("dev.aiphone.input.INPUT"), "android.permission.DUMP", null, Context.RECEIVER_EXPORTED);
    }
    @Override public View onCreateInputView() {
        TextView label = new TextView(this);
        label.setText("");
        label.setHeight(1);
        return label;
    }
    @Override public void onDestroy() { unregisterReceiver(input); super.onDestroy(); }
}
