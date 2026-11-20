extern crate upstream;

mod tasks {
    pub async fn pending(message: String) {
        std::future::pending::<()>().await;
        drop(message);
    }
}

mod first {
    pub fn evaluate(value: u32) -> bool {
        let _future = super::tasks::pending(String::new());
        upstream::apply(value, |number| number >= 3)
    }
}

mod second {
    pub fn evaluate(value: u32) -> bool {
        let _future = super::tasks::pending(String::new());
        upstream::apply(value, |number| number % 2 == 0)
    }
}

fn main() {
    let wrapper = upstream::Wrapper(|| print!("dropped:"));
    drop(wrapper);
    println!(
        "{}:{}:{}:{}",
        first::evaluate(2),
        second::evaluate(2),
        upstream::identity(7_u64),
        upstream::shared_identity(),
    );
}
