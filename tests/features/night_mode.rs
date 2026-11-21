#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use crate::features::simulated_esp32::{
        Application, build_application, display, local_epoch, run_application, time, websocket,
    };
    use embassy_executor::Spawner;
    use embedded_graphics::pixelcolor::Rgb565;

    #[init]
    fn init() -> Application {
        build_application(local_epoch(20, 0, 0))
    }

    fn initialize_before_night() -> Application {
        build_application(local_epoch(16, 0, 0))
    }

    fn initialize_before_day() -> Application {
        build_application(local_epoch(7, 0, 0))
    }

    async fn start(application: Application) -> display::Frame {
        websocket::send_handshake().await;
        websocket::send_event(r#"{"event":"TaskReminder","task_id":7,"task_name":"Take a walk","scheduled_time":"18:00"}"#).await;
        let spawner = unsafe { Spawner::for_current_executor().await };
        run_application(spawner, application);
        display::wait_for_frame(0).await
    }

    fn assert_night(frame: &display::Frame) {
        defmt::assert!(frame.background == Rgb565::new(0, 0, 0));
        defmt::assert!(frame.foreground == Rgb565::new(18, 12, 2));
        defmt::assert!(frame.notification == "Take a walk");
    }

    fn assert_day(frame: &display::Frame) {
        defmt::assert!(frame.background == Rgb565::new(16, 4, 20));
        defmt::assert!(frame.foreground == Rgb565::new(63, 63, 63));
        defmt::assert!(frame.notification == "Take a walk");
    }

    #[test]
    #[timeout(15)]
    async fn user_sees_correct_mode_after_device_synchronizes_its_clock(application: Application) {
        let rtc = application.rtc;
        let frame = start(application).await;
        time::TIME_REQUESTED.wait().await;
        let rtc_seconds = rtc.lock().await.current_time_us() / 1_000_000;
        let synchronized_epoch = u64::from(local_epoch(20, 0, 0));
        defmt::assert!(rtc_seconds >= synchronized_epoch);
        defmt::assert!(rtc_seconds <= synchronized_epoch + 5);
        assert_night(&frame);
    }

    #[test(init = initialize_before_night)]
    #[timeout(15)]
    async fn user_sees_display_switch_to_night_mode(application: Application) {
        let rtc = application.rtc;
        let previous = start(application).await;
        assert_day(&previous);
        time::set_time(rtc, local_epoch(17, 0, 0)).await;
        let current = display::wait_for_frame(previous.number).await;
        assert_night(&current);
        defmt::assert!(previous.notification == current.notification);
    }

    #[test(init = initialize_before_day)]
    #[timeout(15)]
    async fn user_sees_display_switch_to_day_mode(application: Application) {
        let rtc = application.rtc;
        let previous = start(application).await;
        assert_night(&previous);
        time::set_time(rtc, local_epoch(8, 0, 0)).await;
        let current = display::wait_for_frame(previous.number).await;
        assert_day(&current);
        defmt::assert!(previous.notification == current.notification);
    }
}
