package dev.aiphone.input;

import android.graphics.Rect;
import android.util.Xml;
import android.view.accessibility.AccessibilityNodeInfo;
import java.io.IOException;
import java.io.StringWriter;
import org.xmlpull.v1.XmlSerializer;

/** Serializes one live accessibility root in the UIAutomator hierarchy format. */
final class UiHierarchy {
    private int remaining=1500;
    private final XmlSerializer xml=Xml.newSerializer();

    static String capture(AccessibilityNodeInfo root) throws IOException {
        UiHierarchy tree=new UiHierarchy();
        StringWriter output=new StringWriter();
        tree.xml.setOutput(output);
        tree.xml.startDocument("UTF-8",true);
        tree.xml.startTag(null,"hierarchy");
        tree.node(root,0);
        tree.xml.endTag(null,"hierarchy");
        tree.xml.endDocument();
        return output.toString();
    }

    private void node(AccessibilityNodeInfo node,int depth) throws IOException {
        if(--remaining<0||depth>48)throw new IOException("Hierarchy limit exceeded");
        xml.startTag(null,"node");
        attributes(node);
        for(int i=0;i<node.getChildCount();i++) {
            AccessibilityNodeInfo child=node.getChild(i);
            if(child==null)continue;
            try { node(child,depth+1); } finally { child.recycle(); }
        }
        xml.endTag(null,"node");
    }

    private void attributes(AccessibilityNodeInfo node) throws IOException {
        attr("package",node.getPackageName());
        attr("class",node.getClassName());
        attr("resource-id",node.getViewIdResourceName());
        attr("text",node.isPassword()?null:node.getText());
        attr("content-desc",node.isPassword()?null:node.getContentDescription());
        flag("enabled",node.isEnabled());
        flag("visible-to-user",node.isVisibleToUser());
        flag("clickable",node.isClickable());
        flag("scrollable",node.isScrollable());
        flag("editable",node.isEditable());
        flag("password",node.isPassword());
        Rect b=new Rect();
        node.getBoundsInScreen(b);
        attr("bounds","["+b.left+","+b.top+"]["+b.right+","+b.bottom+"]");
    }

    private void flag(String name,boolean value) throws IOException { attr(name,String.valueOf(value)); }
    private void attr(String name,CharSequence value) throws IOException {
        String text=value==null?"":value.toString();
        xml.attribute(null,name,text.substring(0,Math.min(text.length(),512)));
    }
}
