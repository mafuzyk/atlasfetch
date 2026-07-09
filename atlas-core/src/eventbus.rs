pub struct EventBus;

impl EventBus {
    pub fn new() -> Self {
        EventBus
    }

    pub fn on<F, E>(&mut self, _event_type: &str, _handler: F)
    where
        F: FnMut(&E) + 'static,
        E: 'static,
    {
    }

    pub fn emit<E: 'static>(&mut self, _event: &E) {
    }
}
