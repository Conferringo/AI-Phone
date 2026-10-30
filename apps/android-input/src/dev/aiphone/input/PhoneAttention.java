package dev.aiphone.input;

import android.accessibilityservice.AccessibilityService;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Rect;
import android.os.Handler;
import android.os.Looper;
import android.util.AtomicFile;
import android.view.Display;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.List;

/** Passive WeChat activity feed. It never clicks, types, dismisses, or sends messages. */
public final class PhoneAttention extends AccessibilityService {
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final List<JSONObject> events=new ArrayList<>();
    private AtomicFile file;
    private boolean connected, capturing;
    private String lastChat="";
    private final boolean[] badges={false,false};
    private final Runnable scan=()->inspect();
    private final Runnable heartbeat=new Runnable(){public void run(){persist();inspect();handler.postDelayed(this,20000);}};
    @Override protected void onServiceConnected(){
        handler.removeCallbacksAndMessages(null);events.clear();capturing=false;
        file=new AtomicFile(new File(getFilesDir(),"attention.json"));
        try {
            JSONObject saved=new JSONObject(new String(file.readFully(),StandardCharsets.UTF_8));
            JSONArray old=saved.optJSONArray("notifications");
            if(old!=null)for(int i=Math.max(0,old.length()-80);i<old.length();i++)events.add(old.getJSONObject(i));
            badges[0]=saved.optBoolean("chat_badge");badges[1]=saved.optBoolean("contacts_badge");lastChat=saved.optString("chat_signature");
        }catch(Exception ignored){}
        connected=true;handler.post(heartbeat);
    }
    @Override public void onAccessibilityEvent(AccessibilityEvent event){
        if(!"com.tencent.mm".contentEquals(event.getPackageName()==null?"":event.getPackageName()))return;
        handler.removeCallbacks(scan);handler.postDelayed(scan,700);
    }
    @Override public void onInterrupt(){}
    @Override public boolean onUnbind(Intent intent){connected=false;handler.removeCallbacksAndMessages(null);persist();return super.onUnbind(intent);}
    @Override public void onDestroy(){connected=false;handler.removeCallbacksAndMessages(null);persist();super.onDestroy();}
    private static String text(CharSequence value){return value==null?"":value.toString();}
    private void inspect(){
        AccessibilityNodeInfo root=getRootInActiveWindow();
        if(root==null)return;
        try {
            if(!"com.tencent.mm".equals(text(root.getPackageName())))return;
            List<AccessibilityNodeInfo> nodes=new ArrayList<>();collect(root,nodes,0);
            try {
                int h=getResources().getDisplayMetrics().heightPixels;
                Rect[] tabs=new Rect[2];boolean editing=false,password=false;StringBuilder visible=new StringBuilder();
                for(AccessibilityNodeInfo n:nodes){
                    Rect b=new Rect();n.getBoundsInScreen(b);if(!n.isVisibleToUser())continue;
                    if(n.isEditable()){editing=true;password|=n.isPassword();continue;}
                    String label=text(n.getText());
                    if(b.centerY()>h*0.75&&("微信".equals(label)||"通讯录".equals(label)))tabs["微信".equals(label)?0:1]=b;
                    // Text is hashed locally; private chat content is never copied into the attention feed.
                    if(!label.isEmpty()&&!label.matches("\\d{1,2}:\\d{2}")&&!label.contains("正在输入"))visible.append(label).append('\n');
                }
                if(tabs[0]!=null&&tabs[1]!=null&&tabs[0].centerX()<tabs[1].centerX()&&Math.abs(tabs[0].centerY()-tabs[1].centerY())<24){
                    if(!capturing){capturing=true;takeScreenshot(Display.DEFAULT_DISPLAY,getMainExecutor(),new TakeScreenshotCallback(){
                        public void onSuccess(ScreenshotResult result){
                            try {
                                AccessibilityNodeInfo current=getRootInActiveWindow();
                                if(current==null)return;
                                try{if(!"com.tencent.mm".equals(text(current.getPackageName())))return;}finally{current.recycle();}
                                Bitmap raw=Bitmap.wrapHardwareBuffer(result.getHardwareBuffer(),result.getColorSpace());
                                if(raw==null)return;
                                Bitmap bitmap=raw.copy(Bitmap.Config.ARGB_8888,false);raw.recycle();
                                try{for(int i=0;i<2;i++){boolean unread=redBadge(bitmap,tabs[i]);if(unread&&!badges[i])emit("wechat-badge:"+i,i==0?"微信未读":"通讯录未读","底部"+(i==0?"微信":"通讯录")+"出现未读标记，请打开核对。");badges[i]=unread;}}finally{bitmap.recycle();}
                                persist();
                            }finally{result.getHardwareBuffer().close();capturing=false;}
                        }
                        public void onFailure(int error){capturing=false;}
                    });}
                }else if(editing&&!password&&!visible.toString().isEmpty()&&!visible.toString().contains("找回密码")){
                    String signature=hash(visible.toString());
                    if(!lastChat.isEmpty()&&!lastChat.equals(signature))emit("wechat-visible:chat","微信会话变化","当前微信可见内容发生变化，请重新观察确认是否有新消息；不保证是对方的新消息。");
                    lastChat=signature;persist();
                }
            }finally{for(AccessibilityNodeInfo n:nodes)n.recycle();}
        }catch(Exception ignored){}finally{root.recycle();}
    }
    private void collect(AccessibilityNodeInfo node,List<AccessibilityNodeInfo> nodes,int depth){
        if(depth>35||nodes.size()>=500)return;
        nodes.add(AccessibilityNodeInfo.obtain(node));
        for(int i=0;i<node.getChildCount();i++){AccessibilityNodeInfo child=node.getChild(i);if(child!=null){collect(child,nodes,depth+1);child.recycle();}}
    }
    private boolean redBadge(Bitmap bitmap,Rect label){
        float scale=getResources().getDisplayMetrics().density;
        int left=Math.max(0,label.centerX()-(int)(32*scale)),right=Math.min(bitmap.getWidth(),label.centerX()+(int)(32*scale));
        int top=Math.max(0,label.top-(int)(34*scale)),bottom=Math.min(bitmap.getHeight(),label.top+2),red=0;
        for(int y=top;y<bottom;y++)for(int x=left;x<right;x++){int p=bitmap.getPixel(x,y),r=(p>>16)&255,g=(p>>8)&255,b=p&255;if(r>180&&g<115&&b<135&&r>g*1.7&&r>b*1.4)red++;}
        return red>12*scale*scale;
    }
    private static String hash(String value){try{StringBuilder out=new StringBuilder();for(byte b:MessageDigest.getInstance("SHA-256").digest(value.getBytes(StandardCharsets.UTF_8)))out.append(String.format("%02x",b&255));return out.toString();}catch(Exception e){return "";}}
    private void emit(String key,String title,String body){try{long now=System.currentTimeMillis();JSONObject event=new JSONObject();event.put("event_id",hash(key+"\n"+now));event.put("notification_key",key);event.put("package_name","com.tencent.mm");event.put("app_name","微信");event.put("title",title);event.put("text",body);event.put("posted_at_ms",now);events.add(event);while(events.size()>80)events.remove(0);}catch(Exception ignored){}}
    private void persist(){if(file==null)return;FileOutputStream stream=null;try{JSONObject value=new JSONObject();value.put("connected",connected);value.put("checked_at_ms",System.currentTimeMillis());value.put("chat_badge",badges[0]);value.put("contacts_badge",badges[1]);value.put("chat_signature",lastChat);value.put("notifications",new JSONArray(events));stream=file.startWrite();stream.write(value.toString().getBytes(StandardCharsets.UTF_8));file.finishWrite(stream);}catch(Exception ignored){if(stream!=null)file.failWrite(stream);}}
}
