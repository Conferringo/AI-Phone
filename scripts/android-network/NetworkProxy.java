package dev.aiphone.network;

import android.net.ProxyInfo;
import android.os.IBinder;
import java.util.Arrays;

/** Root-only runtime proxy configuration. No app, activity, or launcher entry. */
public final class NetworkProxy {
    public static void main(String[] args) throws Exception {
        Class<?> manager = Class.forName("android.os.ServiceManager");
        IBinder binder = (IBinder) manager.getMethod("getService", String.class).invoke(null, "connectivity");
        Class<?> api = Class.forName("android.net.IConnectivityManager");
        Object service = Class.forName("android.net.IConnectivityManager$Stub")
            .getMethod("asInterface", IBinder.class).invoke(null, binder);
        boolean clear = args.length == 1 && "--clear".equals(args[0]);
        if (clear) {
            api.getMethod("setGlobalProxy", ProxyInfo.class).invoke(service, new Object[] { null });
        } else if (args.length == 3) {
            ProxyInfo proxy = ProxyInfo.buildDirectProxy(args[0], Integer.parseInt(args[1]), Arrays.asList(args[2].split(",")));
            api.getMethod("setGlobalProxy", ProxyInfo.class).invoke(service, proxy);
        } else if (args.length != 0) {
            throw new IllegalArgumentException("Expected host, port, exclusions or no arguments for readback");
        }
        ProxyInfo current = (ProxyInfo) api.getMethod("getGlobalProxy").invoke(service);
        System.out.println(current == null ? "RUNTIME_PROXY=none" : "RUNTIME_PROXY=" + current.getHost() + ":" + current.getPort());
        if (clear && current != null) throw new IllegalStateException("Runtime proxy was not cleared");
        if (args.length == 3 && (current == null || !args[0].equals(current.getHost()) || Integer.parseInt(args[1]) != current.getPort())) {
            throw new IllegalStateException("Runtime proxy did not persist");
        }
    }
}
