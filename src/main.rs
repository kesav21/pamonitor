use futures::executor::block_on;
use futures::future::poll_fn as future_poll_fn;
use futures::future::ready;
use futures::stream::poll_fn as stream_poll_fn;
use futures::stream::StreamExt;
use pulse::callbacks::ListResult;
use pulse::context::introspect::Introspector;
use pulse::context::introspect::SinkInfo;
use pulse::context::subscribe::Facility;
use pulse::context::subscribe::InterestMaskSet;
use pulse::context::subscribe::Operation;
use pulse::context::Context;
use pulse::context::FlagSet as ContextFlagSet;
use pulse::context::State as ContextState;
use pulse::def::Retval;
use pulse::mainloop::standard::Mainloop;
use pulse::proplist::properties::APPLICATION_NAME;
use pulse::proplist::Proplist;
use std::env::var;
use std::fmt::Display;
use std::fmt::Error as FormatError;
use std::fmt::Formatter;
use std::fs::remove_file;
use std::fs::write;
use std::process::Command;
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender;
use std::task::Poll;

struct CustomSinkInfo {
    index: u32,
    mute: bool,
    volume: i32,
    description: String,
}

impl Display for CustomSinkInfo {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
        write!(
            formatter,
            "{}, {}, {}, {}",
            self.index, self.mute, self.volume, self.description
        )
    }
}

impl From<&SinkInfo<'_>> for CustomSinkInfo {
    fn from(sink: &SinkInfo) -> Self {
        let volume = (sink.volume.avg().0 as f64 / 65536.0 * 100.0).round() as i32;
        let description = sink
            .description
            .as_ref()
            .expect("No description")
            .to_string();
        Self {
            index: sink.index,
            mute: sink.mute,
            volume,
            description,
        }
    }
}

enum Message {
    NewSinkInfo(CustomSinkInfo),
    OldSinkInfo(CustomSinkInfo),
    NewestSink,
    SwitchSinkInput(u32, u32),
    NewSinkIndex(u32),
    RemoveSinkIndex(u32),
    ChangeSinkIndex(u32),
    ChangeVolume(CustomSinkInfo),
}

impl Display for Message {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError> {
        match self {
            Message::NewSinkInfo(sink) => write!(formatter, "NewSinkInfo({})", sink),
            Message::OldSinkInfo(sink) => write!(formatter, "OldSinkInfo({})", sink),
            Message::ChangeVolume(sink) => write!(formatter, "ChangeVolume({})", sink),
            Message::NewSinkIndex(sink_index) => {
                write!(formatter, "NewSinkIndex({})", sink_index)
            }
            Message::RemoveSinkIndex(sink_index) => {
                write!(formatter, "RemoveSinkIndex({})", sink_index)
            }
            Message::ChangeSinkIndex(sink_index) => {
                write!(formatter, "ChangeSinkIndex({})", sink_index)
            }
            Message::NewestSink => write!(formatter, "NewestSink"),
            Message::SwitchSinkInput(sink_index, sink_input_index) => {
                write!(
                    formatter,
                    "SwitchSinkInput({}, {})",
                    sink_index, sink_input_index
                )
            }
        }
    }
}

fn list_sinks(sender: Sender<Message>, introspector: &Introspector) {
    introspector.get_sink_info_list(move |result| match result {
        ListResult::Error => eprintln!("Error fetching sinks"),
        ListResult::End => sender
            .send(Message::NewestSink)
            .expect("Failed to report newest sink"),
        ListResult::Item(sink) => sender
            .send(Message::NewSinkInfo(CustomSinkInfo::from(sink)))
            .expect("Failed to report new sink info"),
    });
}

fn list_sinks_old(sender: Sender<Message>, introspector: &Introspector) {
    introspector.get_sink_info_list(move |result| match result {
        ListResult::Error => eprintln!("Error fetching sinks"),
        ListResult::End => sender
            .send(Message::NewestSink)
            .expect("Failed to report newest sink"),
        ListResult::Item(sink) => sender
            .send(Message::OldSinkInfo(CustomSinkInfo::from(sink)))
            .expect("Failed to report new sink info"),
    });
}

fn get_sink_new(sender: Sender<Message>, introspector: &Introspector, index: u32) {
    introspector.get_sink_info_by_index(index, move |result| match result {
        ListResult::Error => eprintln!("Error fetching sink"),
        ListResult::End => sender
            .send(Message::NewestSink)
            .expect("Failed to report newest sink"),
        ListResult::Item(sink) => sender
            .send(Message::NewSinkInfo(CustomSinkInfo::from(sink)))
            .expect("Failed to report new sink info"),
    });
}

fn get_sink_change(sender: Sender<Message>, introspector: &Introspector, sink: &CustomSinkInfo) {
    let sink_mute = sink.mute;
    let sink_volume = sink.volume;
    introspector.get_sink_info_by_index(sink.index, move |result| match result {
        ListResult::Error => eprintln!("Error fetching sink"),
        ListResult::End => {}
        ListResult::Item(new_sink) => {
            let new_sink = CustomSinkInfo::from(new_sink);
            if sink_mute != new_sink.mute || sink_volume != new_sink.volume {
                sender
                    .send(Message::ChangeVolume(new_sink))
                    .expect("Failed to report change in volume");
            }
        }
    });
}

fn list_sink_inputs(sender: Sender<Message>, introspector: &Introspector, sink_index: u32) {
    introspector.get_sink_input_info_list(move |result| match result {
        ListResult::Error => eprintln!("Error fetch sink input"),
        ListResult::End => {}
        ListResult::Item(sink_input) => sender
            .send(Message::SwitchSinkInput(sink_input.index, sink_index))
            .expect("Failed to report sink input"),
    });
}

fn switch_sink_inputs(introspector: &mut Introspector, sink_input_index: u32, sink_index: u32) {
    introspector.move_sink_input_by_index(
        sink_input_index,
        sink_index,
        Some(Box::new(|success| {
            if !success {
                eprintln!("Failed to move sink input")
            }
        })),
    );
}

fn subscribe(sender: Sender<Message>, context: &mut Context) {
    context.subscribe(InterestMaskSet::SINK, |success| {
        if !success {
            eprintln!("Failed to subscribe to context")
        }
    });
    context.set_subscribe_callback(Some(Box::new(move |facility, operation, index| {
        if let Facility::Sink = facility.expect("No facility given") {
            match operation.expect("No operation given") {
                Operation::New => sender
                    .send(Message::NewSinkIndex(index))
                    .expect("Failed to report new sink"),
                Operation::Removed => sender
                    .send(Message::RemoveSinkIndex(index))
                    .expect("Failed to report removed sink"),
                Operation::Changed => {
                    sender
                        .send(Message::ChangeSinkIndex(index))
                        .expect("Failed to report change in sink");
                }
            }
        }
    })));
}

/// TODO: use std::path::Path::new(path).exists()
fn write_data(index: u32, name: &str, value: &str) -> std::io::Result<()> {
    let config = var("XDG_CACHE_HOME").expect("Cannot get XDG_CACHE_HOME");
    let path = format!("{}/bin/pamonitor.{}.{}", config, index, name);
    write(path, value)
}

fn write_sink(sink: &CustomSinkInfo) {
    write_data(sink.index, "mute", &format!("{}", sink.mute)).expect("Failed to write mute");
    write_data(sink.index, "volume", &format!("{}", sink.volume)).expect("Failed to write volume");
    write_data(sink.index, "description", &sink.description).expect("Failed to write description");
}

fn write_newest_sink(index: u32) {
    let config = var("XDG_CACHE_HOME").expect("Cannot get XDG_CACHE_HOME");
    let path = format!("{}/bin/pamonitor.newest_sink_index", config);
    write(path, &format!("{}", index)).expect("Failed to write newest_sink_index");
}

fn delete_sink(index: u32) {
    let config = var("XDG_CACHE_HOME").expect("Cannot get XDG_CACHE_HOME");
    remove_file(format!("{}/bin/pamonitor.{}.mute", config, index)).expect("Failed to delete mute");
    remove_file(format!("{}/bin/pamonitor.{}.volume", config, index))
        .expect("Failed to delete volume");
    remove_file(format!("{}/bin/pamonitor.{}.description", config, index))
        .expect("Failed to delete description");
}

fn notify_sink(sink: &CustomSinkInfo) {
    let status = Command::new("dunstify")
        .arg("--hints")
        .arg("string:x-dunst-stack-tag:change_sink")
        .arg("--timeout")
        .arg("2000")
        .arg("New sink")
        .arg(&sink.description)
        .status()
        .expect("Failed to execute notify-sink command");
    if !status.success() {
        eprintln!("Failed to notify sink change");
    }
}

fn notify_volume(sink: &CustomSinkInfo) {
    let icon = if sink.mute {
        "notification-audio-volume-muted"
    } else {
        "notification-audio-volume-high"
    };
    let status = Command::new("dunstify")
        .arg("--hints")
        .arg(format!("int:value:{}", sink.volume))
        .arg("--hints")
        .arg("string:x-dunst-stack-tag:change_volume")
        .arg("--timeout")
        .arg("1000")
        .arg("--icon")
        .arg(icon)
        .arg("Volume")
        .status()
        .expect("Failed to execute notify command");
    if !status.success() {
        eprintln!("Failed to notify volume change");
    }
}

fn handle_message(
    sender: Sender<Message>,
    introspector: &mut Introspector,
    message: Message,
    newest_sink: &Option<CustomSinkInfo>,
) -> Option<CustomSinkInfo> {
    println!("{}", message);
    match message {
        Message::NewSinkInfo(sink) => {
            write_sink(&sink);
            Some(sink)
        }
        Message::OldSinkInfo(sink) => Some(sink),
        Message::NewestSink => {
            let sink = newest_sink.as_ref().expect("No newest sink");
            write_newest_sink(sink.index);
            notify_sink(&sink);
            list_sink_inputs(sender, &introspector, sink.index);
            None
        }
        Message::SwitchSinkInput(sink_input_index, sink_index) => {
            switch_sink_inputs(introspector, sink_input_index, sink_index);
            None
        }
        Message::NewSinkIndex(index) => {
            get_sink_new(sender, &introspector, index);
            None
        }
        Message::RemoveSinkIndex(index) => {
            list_sinks_old(sender, &introspector);
            delete_sink(index);
            None
        }
        Message::ChangeSinkIndex(index) => {
            let newest_sink = newest_sink.as_ref().expect("No newest sink");
            if newest_sink.index == index {
                get_sink_change(sender, &introspector, newest_sink);
            }
            None
        }
        Message::ChangeVolume(sink) => {
            write_sink(&sink);
            notify_volume(&sink);
            Some(sink)
        }
    }
}

fn main() {
    let mut mainloop = Mainloop::new().expect("Failed to create mainloop");
    let mut proplist = Proplist::new().expect("Failed to create proplist");
    proplist
        .set_str(APPLICATION_NAME, "PAMonitor")
        .expect("Failed to set application name");
    let mut context = Context::new_with_proplist(&mainloop, "PAMonitorContext", &proplist)
        .expect("Failed to create new context");
    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .expect("Failed to connect context");

    block_on(
        // TODO: use the same stream_poll_fn result in both places
        stream_poll_fn(|_| Poll::Ready(Some(mainloop.iterate(false))))
            .take_until(future_poll_fn(|_| match context.get_state() {
                ContextState::Ready => Poll::Ready(()),
                // TODO: handle these edge cases explicitly
                _ => Poll::Pending,
            }))
            .collect::<Vec<_>>(),
    );

    let (sender, receiver) = channel();
    let mut introspector = context.introspect();
    list_sinks(sender.clone(), &introspector);
    subscribe(sender.clone(), &mut context);

    block_on(
        stream_poll_fn(|_| Poll::Ready(Some(mainloop.iterate(false))))
            .map(|_| receiver.try_recv())
            .filter_map(|result| match result {
                // TODO: handle these edge cases explicitly
                Err(_) => ready(None),
                Ok(message) => ready(Some(message)),
            })
            .fold(None, |newest_sink, message| {
                match handle_message(sender.clone(), &mut introspector, message, &newest_sink) {
                    Some(sink) => ready(Some(sink)),
                    None => ready(newest_sink),
                }
            }),
    );

    // Clean shutdown, uncertain whether this is necessary
    mainloop.quit(Retval(0));
}
