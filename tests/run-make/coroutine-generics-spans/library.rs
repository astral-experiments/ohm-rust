pub async fn write(input: &u8) {
    let saved = [7u8; 16];
    std::future::pending::<()>().await;
    std::hint::black_box((input, &saved));
}
