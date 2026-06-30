package androidx.core.app;

import android.annotation.TargetApi;
import android.app.Activity;
import android.app.AppComponentFactory;
import android.app.Application;
import android.app.Service;
import android.content.BroadcastReceiver;
import android.content.ContentProvider;
import android.content.Intent;

@TargetApi(28)
public class CoreComponentFactory extends AppComponentFactory {
    public interface CompatWrapped {
        Object getWrapper();
    }

    @Override
    public Application instantiateApplication(ClassLoader classLoader, String className)
            throws InstantiationException, IllegalAccessException, ClassNotFoundException {
        return checkCompatWrapper(super.instantiateApplication(classLoader, className));
    }

    @Override
    public Activity instantiateActivity(ClassLoader classLoader, String className, Intent intent)
            throws InstantiationException, IllegalAccessException, ClassNotFoundException {
        return checkCompatWrapper(super.instantiateActivity(classLoader, className, intent));
    }

    @Override
    public BroadcastReceiver instantiateReceiver(ClassLoader classLoader, String className, Intent intent)
            throws InstantiationException, IllegalAccessException, ClassNotFoundException {
        return checkCompatWrapper(super.instantiateReceiver(classLoader, className, intent));
    }

    @Override
    public Service instantiateService(ClassLoader classLoader, String className, Intent intent)
            throws InstantiationException, IllegalAccessException, ClassNotFoundException {
        return checkCompatWrapper(super.instantiateService(classLoader, className, intent));
    }

    @Override
    public ContentProvider instantiateProvider(ClassLoader classLoader, String className)
            throws InstantiationException, IllegalAccessException, ClassNotFoundException {
        return checkCompatWrapper(super.instantiateProvider(classLoader, className));
    }

    @SuppressWarnings("unchecked")
    static <T> T checkCompatWrapper(T component) {
        if (component instanceof CompatWrapped) {
            Object wrapper = ((CompatWrapped) component).getWrapper();
            if (wrapper != null) {
                return (T) wrapper;
            }
        }
        return component;
    }
}
