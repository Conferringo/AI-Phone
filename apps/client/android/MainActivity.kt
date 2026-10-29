package dev.aiphone.client

import android.os.Bundle
import android.net.Uri
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    // One WebView: a native Back gesture returns to the trusted connection page.
    // Remote pages receive no command capability and cannot replace this handler.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val current = Uri.parse(webView.url ?: "")
        if (current.host == "tauri.localhost") {
          moveTaskToBack(true)
        } else {
          webView.loadUrl("http://tauri.localhost/")
        }
      }
    })
  }
}
