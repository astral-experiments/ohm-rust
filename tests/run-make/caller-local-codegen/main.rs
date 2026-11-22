extern crate upstream;

struct Receiver;

mod tasks {
    pub fn wrapper() -> upstream::Wrapper<impl Fn()> {
        upstream::Wrapper(|| print!("dropped:"))
    }

    pub async fn pending(message: String) {
        std::future::pending::<()>().await;
        drop(message);
    }
}

mod first {
    impl super::Receiver {
        pub fn evaluate_first(value: u32) -> bool {
            let _future = super::tasks::pending(String::new());
            upstream::apply(value, |number| number >= 3)
        }
    }
}

mod second {
    pub fn number() -> u64 {
        upstream::identity(7_u64)
    }

    pub fn evaluate(value: u32) -> bool {
        let _future = super::tasks::pending(String::new());
        upstream::apply(value, |number| number % 2 == 0)
    }
}

fn main() {
    let wrapper = tasks::wrapper();
    drop(wrapper);
    println!(
        "{}:{}:{}:{}",
        Receiver::evaluate_first(2),
        second::evaluate(2),
        second::number(),
        upstream::shared_identity(),
    );
}
