use std::cell::RefCell;

struct Mark<'a>(&'a RefCell<String>, char);

impl Drop for Mark<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push(self.1);
    }
}

pub fn choose(out: &RefCell<String>, early: bool) {
    let _outer = Mark(out, 'a');
    let (_left, _right) = (Mark(out, 'b'), Mark(out, 'c'));
    {
        let (_left, _right) = (Mark(out, 'd'), Mark(out, 'e'));
        if early {
            return;
        }
    } // inner
    out.borrow_mut().push('!');
} // outer

fn main() {
    let out = RefCell::new(String::new());
    choose(&out, false);
    choose(&out, true);
    assert_eq!(&*out.borrow(), "ed!cbaedcba");
}
