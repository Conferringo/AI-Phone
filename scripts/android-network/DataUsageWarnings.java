package dev.aiphone.network;

import android.os.IBinder;
import java.lang.reflect.Array;
import java.lang.reflect.Field;

/** Root-only emulator setup: disable usage warnings, preserving caps and all other policies. */
public final class DataUsageWarnings {
    public static void main(String[] args) throws Exception {
        boolean disable = args.length == 1 && "--disable-warnings".equals(args[0]);
        if (args.length != 0 && !disable) throw new IllegalArgumentException("Expected --disable-warnings or no arguments for readback");
        IBinder binder = (IBinder) Class.forName("android.os.ServiceManager")
            .getMethod("getService", String.class).invoke(null, "netpolicy");
        Class<?> api = Class.forName("android.net.INetworkPolicyManager");
        Object service = Class.forName("android.net.INetworkPolicyManager$Stub")
            .getMethod("asInterface", IBinder.class).invoke(null, binder);
        Object policies = api.getMethod("getNetworkPolicies", String.class).invoke(service, "android");
        Field warning = Class.forName("android.net.NetworkPolicy").getField("warningBytes");
        Field inferred = Class.forName("android.net.NetworkPolicy").getField("inferred");
        int count = Array.getLength(policies), changed = 0;
        for (int i=0; i<count; i++) {
            Object policy = Array.get(policies, i);
            // Treat this as an explicit preference, so carrier defaults cannot
            // restore the 2 GiB warning when inferred policies are refreshed.
            if (disable && (warning.getLong(policy) != -1L || inferred.getBoolean(policy))) {
                warning.setLong(policy, -1L); inferred.setBoolean(policy, false); changed++;
            }
        }
        if (changed > 0) api.getMethod("setNetworkPolicies", policies.getClass()).invoke(service, policies);
        Object current = api.getMethod("getNetworkPolicies", String.class).invoke(service, "android");
        int enabled = 0, defaults = 0;
        for (int i=0; i<Array.getLength(current); i++) {
            Object policy=Array.get(current, i);
            if (warning.getLong(policy) != -1L) enabled++;
            if (inferred.getBoolean(policy)) defaults++;
        }
        System.out.println("DATA_USAGE_WARNINGS policies="+Array.getLength(current)+" enabled="+enabled+" inferred="+defaults+" changed="+changed);
        if (disable && (enabled != 0 || defaults != 0 || Array.getLength(current) != count)) throw new IllegalStateException("Usage warning readback mismatch");
    }
}
