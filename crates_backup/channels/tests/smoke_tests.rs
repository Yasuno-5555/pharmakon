use pharmakon_channels::{Channel, MockChannel};

#[tokio::test]
async fn test_mock_channel_send() {
    let channel = MockChannel::new("test-mock");
    let result = channel.send("user1", "Hello").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_channel_id() {
    let channel = MockChannel::new("discord-v1");
    assert_eq!(channel.id(), "discord-v1");
}
