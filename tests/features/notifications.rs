#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use crate::features::simulated_esp32::{
        Application, build_application, display, local_epoch, run_application, websocket,
    };
    use embassy_executor::Spawner;

    #[init]
    fn init() -> Application {
        build_application(local_epoch(12, 0, 0))
    }

    #[test]
    #[timeout(15)]
    async fn user_receives_last_notification_on_boot(application: Application) {
        websocket::send_handshake().await;
        websocket::send_event(r#"{"event":"TaskReminder","task_id":7,"task_name":"Take a walk","scheduled_time":"18:00"}"#).await;
        let spawner = unsafe { Spawner::for_current_executor().await };
        run_application(spawner, application);
        let frame = display::wait_for_frame(0).await;
        defmt::assert!(frame.notification == "Take a walk");
        let request = websocket::request();
        let request = core::str::from_utf8(&request).expect("handshake request is UTF-8");
        defmt::assert!(request.starts_with("GET /websocket/notifications HTTP/1.1\r\n"));
        defmt::assert!(request.contains("Authorization: Bearer "));
    }

    #[test]
    #[timeout(15)]
    async fn user_receives_new_notification(application: Application) {
        websocket::send_handshake().await;
        websocket::send_event(r#"{"event":"TaskReminder","task_id":7,"task_name":"Take a walk","scheduled_time":"18:00"}"#).await;
        let spawner = unsafe { Spawner::for_current_executor().await };
        run_application(spawner, application);
        let previous = display::wait_for_frame(0).await;
        websocket::send_event(r#"{"event":"TaskReminder","task_id":8,"task_name":"Drink water","scheduled_time":"19:00"}"#).await;
        let current = display::wait_for_frame(previous.number).await;
        defmt::assert!(previous.notification == "Take a walk");
        defmt::assert!(current.notification == "Drink water");
    }
}
