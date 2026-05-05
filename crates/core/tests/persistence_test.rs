use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::model::Message;
use pharmakon_core::trajectory::{Trajectory, TrajectoryStep};


#[tokio::test]
async fn test_db_persistence_history() {
    let store = DbSessionStore::new("sqlite::memory:").await.unwrap();
    let session_id = "test-session";
    
    let msg = Message {
        role: "user".to_string(),
        content: Some(pharmakon_common::MessageContent::Text("hello".to_string())),
        tool_calls: None,
        tool_call_id: None,
        ..Default::default()
    };
    
    store.save_message(session_id, &msg).await.unwrap();
    
    let history = store.load_history(session_id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content.as_ref().map(|c| c.to_string()), Some("hello".to_string()));
}

#[tokio::test]
async fn test_db_persistence_trajectory() {
    let store = DbSessionStore::new("sqlite::memory:").await.unwrap();
    let session_id = "test-traj-session";
    
    let mut traj = Trajectory::new(session_id.to_string(), "test-model".to_string());
    traj.add_step(TrajectoryStep::Thought { 
        content: "I should say hello".to_string(), 
        timestamp: chrono::Utc::now() 
    });
    
    store.save_trajectory(&traj).await.unwrap();
    
    let loaded = store.load_trajectory(session_id).await.unwrap().unwrap();
    assert_eq!(loaded.steps.len(), 1);
    if let TrajectoryStep::Thought { content, .. } = &loaded.steps[0] {
        assert_eq!(content, "I should say hello");
    } else {
        panic!("Wrong step type");
    }
}
