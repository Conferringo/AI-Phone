package dev.aiphone.diagnostics;
import android.app.Activity;import android.os.*;import android.widget.TextView;import android.view.Gravity;
public final class OrientationProof extends Activity {@Override public void onCreate(Bundle b){super.onCreate(b);TextView t=new TextView(this);t.setText("横屏验证");t.setTextSize(28);t.setGravity(Gravity.CENTER);setContentView(t);new Handler(Looper.getMainLooper()).postDelayed(this::finish,120000);}}
