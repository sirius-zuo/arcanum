use arcanum_middleware::BoundedQueue;

#[tokio::test]
async fn test_push_and_pop() {
    let q: BoundedQueue<i32> = BoundedQueue::new(10);
    q.push(42).await.unwrap();
    let val = q.pop().await;
    assert_eq!(val, Some(42));
}

#[tokio::test]
async fn test_queue_full_returns_error() {
    let q: BoundedQueue<i32> = BoundedQueue::new(2);
    q.push(1).await.unwrap();
    q.push(2).await.unwrap();
    let result = q.push(3).await;
    assert!(result.is_err());
}
