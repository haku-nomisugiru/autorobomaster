package studio.robomaster.console;

import android.Manifest;
import android.app.Activity;
import android.os.Bundle;
import android.os.Build;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.webkit.*;
import android.view.*;
import android.widget.FrameLayout;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.concurrent.Executors;
import java.util.concurrent.ExecutorService;
import org.json.JSONObject;

public class ConsoleActivity extends Activity {
    private WebView web;
    private SharedPreferences prefs;
    private final ExecutorService io=Executors.newSingleThreadExecutor();
    private volatile boolean gone;
    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        prefs=getSharedPreferences("console",MODE_PRIVATE);
        web=new WebView(this);
        FrameLayout frame=new FrameLayout(this);
        frame.setBackgroundColor(0xff101018);
        frame.addView(web);
        frame.setOnApplyWindowInsetsListener((v,insets)->{
            v.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());return insets.consumeSystemWindowInsets();
        });
        setContentView(frame);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setAllowContentAccess(false);
        web.getSettings().setAllowFileAccess(false);
        web.getSettings().setBlockNetworkLoads(true);
        web.setWebChromeClient(new WebChromeClient());
        web.setWebViewClient(new WebViewClient(){
            @Override public boolean shouldOverrideUrlLoading(WebView v,WebResourceRequest r){return true;}
        });
        web.addJavascriptInterface(new ConsoleBridge(),"Console");
        web.loadUrl("file:///android_asset/index.html");
    }
    void notifyJs(String method,String value){
        if(!gone)runOnUiThread(()->{if(!gone)web.evaluateJavascript(method+"("+JSONObject.quote(value)+")",null);});
    }
    private static String origin(String value) throws Exception{
        URI u=new URI(value);
        boolean usb="http".equals(u.getScheme()) && "127.0.0.1".equals(u.getHost()) && u.getPort()>=1024;
        if ((!usb && !"https".equals(u.getScheme())) || u.getHost()==null || u.getUserInfo()!=null || u.getQuery()!=null || u.getFragment()!=null || !("".equals(u.getPath()) || "/".equals(u.getPath()))) throw new IOException("配对地址无效");
        return u.getScheme()+"://"+u.getRawAuthority();
    }
    private JSONObject request(String base,String token,String path,String body) throws Exception{
        HttpURLConnection conn=(HttpURLConnection)new URL(base+"/v1/"+path).openConnection();
        try {
            conn.setInstanceFollowRedirects(false);conn.setConnectTimeout(5500);conn.setReadTimeout(8000);
            conn.setUseCaches(false);conn.setRequestProperty("Authorization","Bearer "+token);
            if(body!=null){
                conn.setRequestMethod("POST");conn.setDoOutput(true);conn.setRequestProperty("Content-Type","application/json");
                try(OutputStream out=conn.getOutputStream()){out.write(body.getBytes(StandardCharsets.UTF_8));}
            }
            int status=conn.getResponseCode();
            InputStream stream=status<400?conn.getInputStream():conn.getErrorStream();
            StringBuilder text=new StringBuilder();
            if(stream!=null)try(Reader reader=new InputStreamReader(stream,StandardCharsets.UTF_8)){
                char[] buf=new char[8192];int n;
                while((n=reader.read(buf))!=-1){if(text.length()+n>16000000)throw new IOException("响应过大");text.append(buf,0,n);}
            }
            JSONObject response=new JSONObject().put("status",status);
            if(status>=200 && status<300)response.put("data",new JSONObject(text.toString()));
            else response.put("error",text.toString());
            return response;
        }finally{conn.disconnect();}
    }
    class ConsoleBridge {
        @JavascriptInterface public void scan(){runOnUiThread(()->{
            if(checkSelfPermission(Manifest.permission.CAMERA)!=PackageManager.PERMISSION_GRANTED)requestPermissions(new String[]{Manifest.permission.CAMERA},1);
            else startActivityForResult(new Intent(ConsoleActivity.this,ScannerActivity.class),20);
        });}
        @JavascriptInterface public void album(){runOnUiThread(()->startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("image/*").addCategory(Intent.CATEGORY_OPENABLE),21));}
        @JavascriptInterface public String device(){return prefs.getString("base","");}
        @JavascriptInterface public String uuid(){return UUID.randomUUID().toString();}
        @JavascriptInterface public String pending(){return prefs.getString("outbox","");}
        @JavascriptInterface public boolean savePending(String value){return prefs.edit().putString("outbox",value).commit();}
        @JavascriptInterface public void forget(){prefs.edit().clear().commit();}
        @JavascriptInterface public void pair(String input){
            io.execute(()->{try{
                JSONObject qr=new JSONObject(input);
                if(qr.getInt("studio")!=1)throw new IOException("不支持的配对码");
                String base=origin(qr.getString("url")),ticket=UUID.fromString(qr.getString("ticket")).toString();
                String token=UUID.randomUUID().toString();
                // Persist before sending so a lost pairing response can be recovered by polling.
                if(!prefs.edit().putString("base",base).putString("token",token).commit())throw new IOException("无法保存设备配对");
                JSONObject response=request(base,token,"pair",new JSONObject().put("ticket",ticket).put("token",token).put("name",Build.MODEL).toString());
                if(response.getInt("status")!=200){
                    prefs.edit().remove("base").remove("token").commit();
                    throw new IOException("配对码无效或已过期，请重新扫码");
                }
                notifyJs("paired","");
            }catch(Exception e){notifyJs("paired",e.getMessage()==null?"连接失败":e.getMessage());}});
        }
        @JavascriptInterface public void call(int seq,String path,String body){
            if(!("state".equals(path)||"operation".equals(path)||"stop".equals(path)||"preview".equals(path)))return;
            final String base=prefs.getString("base",""),token=prefs.getString("token","");
            io.execute(()->{JSONObject result;
                try{result=request(base,token,path,"state".equals(path)?null:body);}
                catch(Exception e){result=new JSONObject();try{result.put("status",0).put("error","连接中断");}catch(Exception ignored){}}
                notifyJs("window.reply.bind(null,"+seq+")",result.toString());
            });
        }
    }
    @Override public void onRequestPermissionsResult(int code,String[] permissions,int[] results){
        super.onRequestPermissionsResult(code,permissions,results);
        if(code==1 && results.length>0 && results[0]==PackageManager.PERMISSION_GRANTED)startActivityForResult(new Intent(this,ScannerActivity.class),20);
        else notifyJs("notice","可在系统设置中允许相机，或从相册选择配对码");
    }
    @Override protected void onActivityResult(int code,int result,Intent intent){
        super.onActivityResult(code,result,intent);
        if(result!=RESULT_OK || intent==null)return;
        if(code==20){notifyJs("scanResult",intent.getStringExtra("qr"));return;}
        if(code==21 && intent.getData()!=null)io.execute(()->{
            try{
                BitmapFactory.Options opt=new BitmapFactory.Options();opt.inJustDecodeBounds=true;
                try(InputStream s=getContentResolver().openInputStream(intent.getData())){BitmapFactory.decodeStream(s,null,opt);}
                opt.inJustDecodeBounds=false;opt.inSampleSize=1;
                while(Math.max(opt.outWidth,opt.outHeight)/opt.inSampleSize>1800)opt.inSampleSize*=2;
                Bitmap bmp;try(InputStream s=getContentResolver().openInputStream(intent.getData())){bmp=BitmapFactory.decodeStream(s,null,opt);}
                notifyJs("scanResult",ScannerActivity.readImage(bmp));
            }catch(Exception e){notifyJs("notice","没有识别到配对二维码");}
        });
    }
    @Override public void onDestroy(){gone=true;io.shutdownNow();web.removeJavascriptInterface("Console");web.destroy();super.onDestroy();}
}

