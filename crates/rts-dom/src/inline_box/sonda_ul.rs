//! Testes movidos de `inline_box.rs` na modularização; nenhuma linha foi
//! alterada. A indentação de 4 espaços é a do `mod` de origem e foi MANTIDA:
//! há literais multi-linha em que o espaço à esquerda é conteúdo.

    use crate::table::tests::geometria;
    fn r(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }
    #[test]
    fn sonda() {
        println!("ul aninhada     = {:?}", r("<ul><li>a<ul><li>b</li></ul></li></ul>", "ul", 1));
        println!("ul vazia        = {:?}", r("<ul><li>a<ul></ul></li></ul>", "ul", 1));
        println!("ul em li inline = {:?}", r("<li style='display:inline'>a<ul><li>b</li></ul></li>", "ul", 0));
        println!("navbox          = {:?}", r("<div><ul><li><ul><li><a>x</a></li></ul></li></ul></div>", "ul", 1));
        println!("ul so espacos   = {:?}", r("<ul><li>a<ul>   </ul></li></ul>", "ul", 1));
    }
