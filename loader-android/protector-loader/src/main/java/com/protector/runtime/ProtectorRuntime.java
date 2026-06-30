package com.protector.runtime;

import android.app.Application;
import android.content.Context;
import android.content.ContextWrapper;
import android.content.pm.ApplicationInfo;
import android.os.Build;
import android.util.Base64;
import dalvik.system.DexClassLoader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import org.json.JSONException;
import org.json.JSONObject;

public final class ProtectorRuntime {
    private static final String ORIGINAL_APPLICATION_KEY = "protector.original_application";
    private static final String PROTECTION_MANIFEST = "protector/protection-manifest.json";
    private static final String DEX_PAYLOAD_METADATA = "protector/dex-payload.json";
    private static volatile boolean initialized;
    private static volatile boolean nativeReady;
    private static volatile ClassLoader payloadClassLoader;

    private ProtectorRuntime() {}

    public static synchronized void init(Context context) {
        if (initialized) {
            return;
        }
        installEncryptedDexPayload(context);
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
            ClassLoader classLoader = payloadClassLoader != null ? payloadClassLoader : context.getClassLoader();
            Class<?> clazz = classLoader.loadClass(className);
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

    private static void installEncryptedDexPayload(Context context) {
        JSONObject metadata = readJsonAsset(context, DEX_PAYLOAD_METADATA);
        if (metadata == null || !metadata.optBoolean("active", false)) {
            return;
        }
        try {
            String payloadAsset = normalizeAssetPath(metadata.optString("payloadFile", "protector/dex-payload.bin"));
            byte[] ciphertext = readAsset(context, payloadAsset);
            byte[] plaintext = decryptPayload(
                    ciphertext,
                    metadata.optString("keyB64", ""),
                    metadata.optString("nonceB64", ""));
            String expectedSha = metadata.optString("plaintextSha256B64", "");
            if (!expectedSha.isEmpty() && !expectedSha.equals(sha256B64(plaintext))) {
                throw new IllegalStateException("Protector DEX payload checksum mismatch");
            }

            File root = new File(context.getCodeCacheDir(), "protector");
            File optimized = new File(root, "optimized");
            if (!optimized.exists() && !optimized.mkdirs()) {
                throw new IOException("Failed to create optimized dex directory: " + optimized);
            }
            File dexZip = new File(root, "original-dex.zip");
            writeFile(dexZip, plaintext);
            if (!dexZip.setReadOnly()) {
                throw new IOException("Failed to mark runtime dex read-only: " + dexZip);
            }

            DexClassLoader loader = new DexClassLoader(
                    dexZip.getAbsolutePath(),
                    optimized.getAbsolutePath(),
                    buildNativeLibraryPath(context),
                    context.getClassLoader());
            payloadClassLoader = loader;
            replaceLoadedApkClassLoader(context, loader);
            Thread.currentThread().setContextClassLoader(loader);
        } catch (IOException | ReflectiveOperationException | RuntimeException error) {
            throw new IllegalStateException("Failed to install encrypted DEX payload", error);
        }
    }

    private static String readOriginalApplicationName(Context context) {
        ApplicationInfo info = context.getApplicationInfo();
        if (info.metaData != null) {
            String configured = info.metaData.getString(ORIGINAL_APPLICATION_KEY);
            if (configured != null && !configured.isEmpty()) {
                return configured;
            }
        }
        JSONObject manifest = readJsonAsset(context, PROTECTION_MANIFEST);
        if (manifest == null) {
            return null;
        }
        JSONObject patch = manifest.optJSONObject("manifestPatch");
        if (patch == null) {
            return null;
        }
        String className = patch.optString("originalApplication", null);
        return className == null || className.isEmpty() ? null : className;
    }

    private static JSONObject readJsonAsset(Context context, String assetPath) {
        try {
            return new JSONObject(new String(readAsset(context, assetPath), StandardCharsets.UTF_8));
        } catch (IOException | JSONException | RuntimeException ignored) {
            return null;
        }
    }

    private static byte[] readAsset(Context context, String assetPath) throws IOException {
        try (InputStream input = context.getAssets().open(assetPath)) {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[8192];
            while (true) {
                int read = input.read(buffer);
                if (read < 0) {
                    break;
                }
                output.write(buffer, 0, read);
            }
            return output.toByteArray();
        }
    }

    private static byte[] decryptPayload(byte[] ciphertext, String keyB64, String nonceB64) {
        try {
            byte[] key = Base64.decode(keyB64, Base64.DEFAULT);
            byte[] nonce = Base64.decode(nonceB64, Base64.DEFAULT);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, new SecretKeySpec(key, "AES"), new GCMParameterSpec(128, nonce));
            return cipher.doFinal(ciphertext);
        } catch (Exception error) {
            throw new IllegalStateException("Failed to decrypt DEX payload", error);
        }
    }

    private static String sha256B64(byte[] bytes) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            return Base64.encodeToString(digest.digest(bytes), Base64.NO_WRAP);
        } catch (NoSuchAlgorithmException error) {
            throw new IllegalStateException("SHA-256 unavailable", error);
        }
    }

    private static void writeFile(File file, byte[] bytes) throws IOException {
        File parent = file.getParentFile();
        if (parent != null && !parent.exists() && !parent.mkdirs()) {
            throw new IOException("Failed to create directory: " + parent);
        }
        if (file.exists() && !file.setWritable(true)) {
            throw new IOException("Failed to make existing file writable: " + file);
        }
        try (FileOutputStream output = new FileOutputStream(file, false)) {
            output.write(bytes);
        }
    }

    private static String normalizeAssetPath(String path) {
        if (path.startsWith("base/assets/")) {
            return path.substring("base/assets/".length());
        }
        if (path.startsWith("assets/")) {
            return path.substring("assets/".length());
        }
        return path;
    }

    private static String buildNativeLibraryPath(Context context) {
        ApplicationInfo info = context.getApplicationInfo();
        StringBuilder builder = new StringBuilder();
        if (info.nativeLibraryDir != null && !info.nativeLibraryDir.isEmpty()) {
            builder.append(info.nativeLibraryDir);
        }
        String sourceDir = info.sourceDir;
        if (sourceDir != null && !sourceDir.isEmpty() && Build.SUPPORTED_ABIS != null) {
            for (String abi : Build.SUPPORTED_ABIS) {
                if (abi == null || abi.isEmpty()) {
                    continue;
                }
                if (builder.length() > 0) {
                    builder.append(File.pathSeparator);
                }
                builder.append(sourceDir).append("!/lib/").append(abi);
            }
        }
        return builder.length() == 0 ? null : builder.toString();
    }

    private static void replaceLoadedApkClassLoader(Context context, ClassLoader loader)
            throws ReflectiveOperationException {
        Object packageInfo = getFieldValue(context, "mPackageInfo");
        if (packageInfo == null && context instanceof ContextWrapper) {
            packageInfo = getFieldValue(((ContextWrapper) context).getBaseContext(), "mPackageInfo");
        }
        if (packageInfo != null) {
            setFieldValue(packageInfo, "mClassLoader", loader);
        }
    }

    private static Object getFieldValue(Object target, String name) throws ReflectiveOperationException {
        if (target == null) {
            return null;
        }
        Field field = findField(target.getClass(), name);
        if (field == null) {
            return null;
        }
        field.setAccessible(true);
        return field.get(target);
    }

    private static void setFieldValue(Object target, String name, Object value) throws ReflectiveOperationException {
        Field field = findField(target.getClass(), name);
        if (field == null) {
            return;
        }
        field.setAccessible(true);
        field.set(target, value);
    }

    private static Field findField(Class<?> type, String name) {
        Class<?> current = type;
        while (current != null) {
            try {
                return current.getDeclaredField(name);
            } catch (NoSuchFieldException ignored) {
                current = current.getSuperclass();
            }
        }
        return null;
    }

    private static native void nativeInit(String packageName, String sourceDir, int sdkInt);

    private static native Object nativeInvoke(int methodId, Object receiver, Object[] args);
}
