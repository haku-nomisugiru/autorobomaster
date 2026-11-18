package studio.robomaster.console;

import android.app.Activity;
import android.os.Bundle;
import android.content.Intent;
import android.graphics.Bitmap;
import android.hardware.Camera;
import android.view.*;
import android.widget.*;
import com.google.zxing.*;
import com.google.zxing.common.HybridBinarizer;
import com.google.zxing.qrcode.QRCodeReader;

@SuppressWarnings("deprecation")
public class ScannerActivity extends Activity implements SurfaceHolder.Callback {
    private Camera camera;
    private boolean done;
    static String readImage(Bitmap bmp) throws Exception{
        if(bmp==null)throw new IllegalArgumentException();
        int w=bmp.getWidth(),h=bmp.getHeight();int[] pixels=new int[w*h];
        bmp.getPixels(pixels,0,w,0,0,w,h);bmp.recycle();
        return decode(new RGBLuminanceSource(w,h,pixels));
    }
    private static String decode(LuminanceSource source) throws Exception{
        return new QRCodeReader().decode(new BinaryBitmap(new HybridBinarizer(source))).getText();
    }
    public void onCreate(Bundle s){
        super.onCreate(s);FrameLayout root=new FrameLayout(this);SurfaceView view=new SurfaceView(this);
        root.addView(view);TextView hint=new TextView(this);hint.setText("将电脑上的配对二维码放入画面");hint.setTextSize(18);hint.setPadding(28,48,28,28);
        hint.setBackgroundColor(0x99000000);hint.setGravity(Gravity.CENTER);
        root.addView(hint,new FrameLayout.LayoutParams(-1,-2,Gravity.BOTTOM));setContentView(root);view.getHolder().addCallback(this);
    }
    public void surfaceCreated(SurfaceHolder holder){try{
        camera=Camera.open();Camera.Parameters p=camera.getParameters();
        if(p.getSupportedFocusModes()!=null && p.getSupportedFocusModes().contains(Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE))p.setFocusMode(Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE);
        camera.setParameters(p);camera.setDisplayOrientation(90);camera.setPreviewDisplay(holder);
        camera.setPreviewCallback((data,cam)->{if(done)return;try{
            Camera.Size size=cam.getParameters().getPreviewSize();
            String qr=decode(new PlanarYUVLuminanceSource(data,size.width,size.height,0,0,size.width,size.height,false));
            if(!qr.contains("\"studio\""))return;
            done=true;setResult(RESULT_OK,new Intent().putExtra("qr",qr));finish();
        }catch(Exception ignored){}});
        camera.startPreview();
    }catch(Exception e){Toast.makeText(this,"相机无法打开，请从相册选择二维码",Toast.LENGTH_LONG).show();finish();}}
    public void surfaceChanged(SurfaceHolder h,int f,int w,int height){}
    public void surfaceDestroyed(SurfaceHolder h){if(camera!=null){camera.setPreviewCallback(null);camera.release();camera=null;}}
}

