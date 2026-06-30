package com.protector.runtime;

import android.app.Application;
import android.content.Context;

public final class ProtectorApplication extends Application {
    private Application delegate;

    @Override
    protected void attachBaseContext(Context base) {
        super.attachBaseContext(base);
        ProtectorRuntime.init(base);
        delegate = ProtectorRuntime.createOriginalApplication(base);
        if (delegate != null) {
            ProtectorRuntime.callAttachBaseContext(delegate, base);
        }
    }

    @Override
    public void onCreate() {
        super.onCreate();
        if (delegate != null) {
            delegate.onCreate();
        }
    }
}

