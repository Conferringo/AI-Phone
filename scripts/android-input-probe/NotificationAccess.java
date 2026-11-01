package dev.aiphone.input.probe;

import android.content.ComponentName;
import android.os.IBinder;
import android.os.Process;

/** Root-only authorization observation for one fixed listener and Android user. */
public final class NotificationAccess {
    public static void main(String[] args) {
        try {
            if (args.length != 0 || Process.myUid() != 0) throw new IllegalStateException();
            IBinder binder = (IBinder) Class.forName("android.os.ServiceManager")
                .getMethod("getService", String.class).invoke(null, "notification");
            if (binder == null) throw new IllegalStateException();
            Class<?> api = Class.forName("android.app.INotificationManager");
            Object service = Class.forName("android.app.INotificationManager$Stub")
                .getMethod("asInterface", IBinder.class).invoke(null, binder);
            Object granted = api.getMethod("isNotificationListenerAccessGrantedForUser",
                ComponentName.class, int.class).invoke(service,
                new ComponentName("dev.aiphone.input", "dev.aiphone.input.PhoneNotifications"), 0);
            if (!(granted instanceof Boolean)) throw new IllegalStateException();
            System.out.println(((Boolean) granted).booleanValue() ? "true" : "false");
        } catch (Throwable error) {
            // No Binder/provider message or other user fields cross this boundary.
            System.err.println("Android input notification observation is unknown");
            System.exit(1);
        }
    }
}
