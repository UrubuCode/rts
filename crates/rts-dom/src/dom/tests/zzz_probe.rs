use super::*;
fn r1(html:&str,sel:&str)->Option<crate::paint::Rect>{
    let dom=parse_html_to_dom(html);
    let ctx=crate::layout::LayoutCtx{viewport_w:1000.0,viewport_h:800.0,measurer:&crate::layout::ApproxMeasurer};
    let list=crate::layout::layout_document(&dom,&ctx);
    dom.query(sel).and_then(|n|dom.resolve(n)).and_then(|n|list.rect_of(n))
}
#[test]
fn zzz_mh(){
    // 021: abspos inset:0 max-height:100 aspect 1/1 in 200-wide CB => clamp h to 100, width from ratio = 100. 100x100.
    eprintln!("021 (100x100) -> {:?}", r1("<div style='position:relative;width:200px;height:300px'><div id='t' style='position:absolute;left:0;right:0;top:0;bottom:0;max-height:100px;aspect-ratio:1/1'></div></div>","#t").map(|r|(r.w,r.h)));
}
