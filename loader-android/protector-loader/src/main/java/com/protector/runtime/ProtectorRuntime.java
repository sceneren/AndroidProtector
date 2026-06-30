package com.protector.runtime;

import android.app.Application;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.os.Build;
import java.lang.reflect.Method;

public final class ProtectorRuntime {
    private static final String ORIGINAL_APPLICATION_KEY = "protector.original_application";
    private static volatile boolean initialized;

    private ProtectorRuntime() {}

    public static synchronized void init(Context context) {
        if (initialized) {
            return;
        }
        System.loadLibrary("protector_vm");
        nativeInit(context.getPackageName(), context.getApplicationInfo().sourceDir, Build.VERSION.SDK_INT);
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
        return nativeInvoke(methodId, receiver, args);
    }

    private static String readOriginalApplicationName(Context context) {
        ApplicationInfo info = context.getApplicationInfo();
        if (info.metaData == null) {
            return null;
        }
        return info.metaData.getString(ORIGINAL_APPLICATION_KEY);
    }

    private static native void nativeInit(String packageName, String sourceDir, int sdkInt);

    private static native Object nativeInvoke(int methodId, Object receiver, Object[] args);
}

