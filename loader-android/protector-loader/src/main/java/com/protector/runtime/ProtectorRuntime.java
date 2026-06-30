package com.protector.runtime;

import android.app.Application;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.os.Build;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import org.json.JSONException;
import org.json.JSONObject;

public final class ProtectorRuntime {
    private static final String ORIGINAL_APPLICATION_KEY = "protector.original_application";
    private static volatile boolean initialized;
    private static volatile boolean nativeReady;

    private ProtectorRuntime() {}

    public static synchronized void init(Context context) {
        if (initialized) {
            return;
        }
        try {
            System.loadLibrary("protector_vm");
            nativeInit(context.getPackageName(), context.getApplicationInfo().sourceDir, Build.VERSION.SDK_INT);
            nativeReady = true;
        } catch (UnsatisfiedLinkError error) {
            nativeReady = false;
        }
        initialized = true;
    }

    public static Application createOriginalApplication(Context context) {
        String className = readOriginalApplicationName(context);
        if (className == null || className.isEmpty() || ProtectorApplication.class.getName().equals(className)) {
            return null;
        }
        try {
            Class<?> clazz = context.getClassLoader().loadClass(className);
            Object instance = clazz.getDeclaredConstructor().newInstance();
            return (Application) instance;
        } catch (ReflectiveOperationException | ClassCastException error) {
            throw new IllegalStateException("Failed to create original Application: " + className, error);
        }
    }

    public static void callAttachBaseContext(Application application, Context context) {
        try {
            Method method = Application.class.getDeclaredMethod("attach", Context.class);
            method.setAccessible(true);
            method.invoke(application, context);
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Failed to attach original Application", error);
        }
    }

    public static Object invokeVm(int methodId, Object receiver, Object[] args) {
        if (!nativeReady) {
            throw new IllegalStateException("Protector native VM is unavailable");
        }
        return nativeInvoke(methodId, receiver, args);
    }

    private static String readOriginalApplicationName(Context context) {
        ApplicationInfo info = context.getApplicationInfo();
        if (info.metaData != null) {
            String configured = info.metaData.getString(ORIGINAL_APPLICATION_KEY);
            if (configured != null && !configured.isEmpty()) {
                return configured;
            }
        }
        return readOriginalApplicationNameFromAssets(context);
    }

    private static String readOriginalApplicationNameFromAssets(Context context) {
        try (InputStream input = context.getAssets().open("protector/protection-manifest.json")) {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[4096];
            while (true) {
                int read = input.read(buffer);
                if (read < 0) {
                    break;
                }
                output.write(buffer, 0, read);
            }
            JSONObject manifest = new JSONObject(output.toString(StandardCharsets.UTF_8.name()));
            JSONObject patch = manifest.optJSONObject("manifestPatch");
            if (patch == null) {
                return null;
            }
            String className = patch.optString("originalApplication", null);
            return className == null || className.isEmpty() ? null : className;
        } catch (IOException | JSONException | RuntimeException ignored) {
            return null;
        }
    }

    private static native void nativeInit(String packageName, String sourceDir, int sdkInt);

    private static native Object nativeInvoke(int methodId, Object receiver, Object[] args);
}
