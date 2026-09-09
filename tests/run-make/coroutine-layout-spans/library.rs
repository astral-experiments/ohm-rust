pub async fn write() {
    let saved = [7u8; 16];
    std::future::pending::<()>().await;
    std::hint::black_box(&saved);
}
