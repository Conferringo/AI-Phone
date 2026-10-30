package dev.aiphone.home;

import android.app.Activity;
import android.content.*;
import android.content.pm.*;
import android.graphics.Color;
import android.graphics.drawable.*;
import android.net.Uri;
import android.os.*;
import android.view.*;
import android.widget.*;
import java.util.*;

/** One canonical icon per installed package, directly on paged home screens. */
public final class HomeActivity extends Activity {
    private final ArrayList<App> apps=new ArrayList<>();
    private LinearLayout root,pages;
    private HorizontalScrollView scroller;
    private final Handler handler=new Handler(Looper.getMainLooper());
    private int page=0;
    private boolean dragging=false;
    private static final class App {
        String key,label; ComponentName component; Drawable icon;
        App(ResolveInfo info,PackageManager pm){component=new ComponentName(info.activityInfo.packageName,info.activityInfo.name);key=component.getPackageName();label=info.loadLabel(pm).toString();icon=info.loadIcon(pm);}
    }
    private final BroadcastReceiver packages=new BroadcastReceiver(){@Override public void onReceive(Context c,Intent i){reload();}};
    private int dp(float value){return Math.round(value*getResources().getDisplayMetrics().density);}
    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        getWindow().setDecorFitsSystemWindows(false);
        getWindow().setStatusBarColor(Color.TRANSPARENT);getWindow().setNavigationBarColor(Color.TRANSPARENT);
        root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);
        root.setBackground(new GradientDrawable(GradientDrawable.Orientation.TL_BR,new int[]{0xff253749,0xff111925,0xff0b1019}));
        root.setOnApplyWindowInsetsListener((v,insets)->{android.graphics.Insets safe=insets.getInsets(WindowInsets.Type.systemBars());int top=safe.top+dp(28),bottom=safe.bottom+dp(16);boolean changed=v.getPaddingLeft()!=safe.left||v.getPaddingTop()!=top||v.getPaddingRight()!=safe.right||v.getPaddingBottom()!=bottom;v.setPadding(safe.left,top,safe.right,bottom);if(changed&&scroller!=null)handler.post(this::render);return insets;});
        setContentView(root);
        IntentFilter filter=new IntentFilter();filter.addAction(Intent.ACTION_PACKAGE_ADDED);filter.addAction(Intent.ACTION_PACKAGE_REMOVED);filter.addAction(Intent.ACTION_PACKAGE_CHANGED);filter.addDataScheme("package");registerReceiver(packages,filter,Context.RECEIVER_EXPORTED);
        reload();
    }
    @Override protected void onResume(){super.onResume();reload();}
    @Override protected void onNewIntent(Intent intent){super.onNewIntent(intent);page=0;if(scroller!=null)scroller.smoothScrollTo(0,0);}
    @Override public void onBackPressed(){if(scroller!=null)scroller.smoothScrollTo(0,0);}
    @Override protected void onDestroy(){unregisterReceiver(packages);handler.removeCallbacksAndMessages(null);super.onDestroy();}
    private void reload(){
        if(dragging)return;
        PackageManager pm=getPackageManager();Intent main=new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
        LinkedHashMap<String,App> unique=new LinkedHashMap<>();
        List<ResolveInfo> found=pm.queryIntentActivities(main,0);found.sort((a,b)->a.activityInfo.name.compareTo(b.activityInfo.name));
        for(ResolveInfo info:found){String pkg=info.activityInfo.packageName;if(!pkg.equals(getPackageName())&&info.activityInfo.enabled)unique.putIfAbsent(pkg,new App(info,pm));}
        ArrayList<String> order=new ArrayList<>(Arrays.asList(getPreferences(0).getString("order","").split("\\|")));
        apps.clear();for(String key:order){App app=unique.remove(key);if(app!=null)apps.add(app);}
        ArrayList<App> added=new ArrayList<>(unique.values());added.sort((a,b)->a.label.compareToIgnoreCase(b.label));apps.addAll(added);saveOrder();render();
    }
    private void saveOrder(){ArrayList<String> keys=new ArrayList<>();for(App app:apps)keys.add(app.key);getPreferences(0).edit().putString("order",String.join("|",keys)).apply();}
    private void render(){
        root.removeAllViews();
        int screen=getResources().getDisplayMetrics().widthPixels-root.getPaddingLeft()-root.getPaddingRight();
        int availableHeight=getResources().getDisplayMetrics().heightPixels-root.getPaddingTop()-root.getPaddingBottom()-dp(24);
        final int columns=Math.max(4,Math.min(8,screen/dp(82))),rows=Math.max(1,Math.min(5,availableHeight/dp(106))),pageSize=columns*rows;
        scroller=new HorizontalScrollView(this){@Override public boolean onTouchEvent(android.view.MotionEvent event){boolean result=super.onTouchEvent(event);if(event.getActionMasked()==MotionEvent.ACTION_UP||event.getActionMasked()==MotionEvent.ACTION_CANCEL){post(()->{page=Math.max(0,Math.min((apps.size()-1)/pageSize,Math.round(getScrollX()/(float)getWidth())));smoothScrollTo(page*getWidth(),0);});}return result;}};
        scroller.setHorizontalScrollBarEnabled(false);scroller.setOverScrollMode(View.OVER_SCROLL_NEVER);scroller.setFillViewport(true);
        pages=new LinearLayout(this);pages.setOrientation(LinearLayout.HORIZONTAL);scroller.addView(pages,new HorizontalScrollView.LayoutParams(-2,-1));
        int count=Math.max(1,(apps.size()+pageSize-1)/pageSize);page=Math.min(page,count-1);
        for(int p=0;p<count;p++){
            final int pageIndex=p;
            GridLayout grid=new GridLayout(this);grid.setColumnCount(columns);grid.setRowCount(rows);grid.setPadding(dp(8),0,dp(8),0);
            pages.addView(grid,new LinearLayout.LayoutParams(screen,-1));
            for(int index=p*pageSize;index<Math.min(apps.size(),(p+1)*pageSize);index++){
                App app=apps.get(index);View icon=icon(app);GridLayout.LayoutParams lp=new GridLayout.LayoutParams();lp.width=(screen-dp(16))/columns;lp.height=dp(106);grid.addView(icon,lp);
            }
            grid.setOnDragListener((v,event)->{
                if(event.getAction()==DragEvent.ACTION_DRAG_STARTED)return event.getLocalState() instanceof App;
                if(event.getAction()==DragEvent.ACTION_DROP){App item=(App)event.getLocalState();int col=Math.max(0,Math.min(columns-1,(int)((event.getX()-dp(8))/((screen-dp(16))/(float)columns))));int row=Math.max(0,Math.min(rows-1,(int)(event.getY()/dp(106))));int destination=Math.min(apps.size()-1,pageIndex*pageSize+row*columns+col);apps.remove(item);apps.add(Math.min(destination,apps.size()),item);saveOrder();return true;}
                if(event.getAction()==DragEvent.ACTION_DRAG_LOCATION&&count>1){if(event.getX()>screen-dp(24)&&page<count-1)scroller.smoothScrollTo((++page)*screen,0);else if(event.getX()<dp(24)&&page>0)scroller.smoothScrollTo((--page)*screen,0);return true;}
                if(event.getAction()==DragEvent.ACTION_DRAG_ENDED){dragging=false;handler.post(this::render);return true;}return true;
            });
        }
        root.addView(scroller,new LinearLayout.LayoutParams(-1,0,1));
        if(count>1){TextView dots=new TextView(this);dots.setText("●"+String.join("",Collections.nCopies(count-1,"  ·")));dots.setTextColor(0xffb4bbc5);dots.setGravity(Gravity.CENTER);dots.setTextSize(12);dots.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);root.addView(dots,new LinearLayout.LayoutParams(-1,dp(24)));}
        scroller.post(()->scroller.scrollTo(page*screen,0));
    }
    private View icon(App app){
        LinearLayout box=new LinearLayout(this);box.setOrientation(LinearLayout.VERTICAL);box.setGravity(Gravity.TOP|Gravity.CENTER_HORIZONTAL);box.setPadding(0,dp(8),0,0);box.setContentDescription(app.label);box.setFocusable(true);box.setClickable(true);
        ImageView image=new ImageView(this);image.setImageDrawable(app.icon);image.setScaleType(ImageView.ScaleType.FIT_CENTER);image.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);box.addView(image,new LinearLayout.LayoutParams(dp(58),dp(58)));
        TextView label=new TextView(this);label.setText(app.label);label.setTextColor(Color.WHITE);label.setTextSize(12);label.setGravity(Gravity.CENTER);label.setMaxLines(2);label.setPadding(dp(2),dp(8),dp(2),0);label.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);box.addView(label,new LinearLayout.LayoutParams(-1,-2));
        box.setOnClickListener(v->open(app));
        box.setOnTouchListener(new View.OnTouchListener(){float x,y;boolean armed=false,moved=false;final Runnable arm=()->{armed=true;box.getParent().requestDisallowInterceptTouchEvent(true);box.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS);};
            public boolean onTouch(View v,MotionEvent e){switch(e.getActionMasked()){
                case MotionEvent.ACTION_DOWN:x=e.getX();y=e.getY();armed=false;moved=false;handler.postDelayed(arm,ViewConfiguration.getLongPressTimeout());v.setAlpha(.72f);return true;
                case MotionEvent.ACTION_MOVE:if(armed&&Math.hypot(e.getX()-x,e.getY()-y)>dp(8)){handler.removeCallbacks(arm);moved=true;dragging=true;v.setAlpha(1);v.startDragAndDrop(ClipData.newPlainText("app",app.key),new View.DragShadowBuilder(image),app,0);}else if(!armed&&Math.hypot(e.getX()-x,e.getY()-y)>dp(12))handler.removeCallbacks(arm);return true;
                case MotionEvent.ACTION_UP:v.getParent().requestDisallowInterceptTouchEvent(false);handler.removeCallbacks(arm);v.setAlpha(1);if(!moved){if(armed)menu(app,box);else if(Math.hypot(e.getX()-x,e.getY()-y)<dp(12))v.performClick();}return true;
                case MotionEvent.ACTION_CANCEL:v.getParent().requestDisallowInterceptTouchEvent(false);handler.removeCallbacks(arm);v.setAlpha(1);return true;
            }return true;}
        });return box;
    }
    private void open(App app){try{startActivity(new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER).setComponent(app.component).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED));}catch(ActivityNotFoundException e){reload();}}
    private void menu(App app,View anchor){PopupMenu menu=new PopupMenu(this,anchor);menu.getMenu().add("打开").setOnMenuItemClickListener(item->{open(app);return true;});menu.getMenu().add("应用信息").setOnMenuItemClickListener(item->{startActivity(new Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,Uri.parse("package:"+app.key)));return true;});if(!app.key.equals("com.android.settings")&&!app.key.equals("com.android.documentsui"))menu.getMenu().add("卸载").setOnMenuItemClickListener(item->{startActivity(new Intent(Intent.ACTION_DELETE,Uri.parse("package:"+app.key)));return true;});menu.show();}
}
