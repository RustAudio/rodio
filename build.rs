use cfg_aliases::cfg_aliases;

fn main() {
    // Setup cfg aliases
    cfg_aliases! {
        symphonia: {
            any(
                feature = "symphonia-mp3",
                feature = "symphonia-wav",
                feature = "symphonia-aac",
                feature = "symphonia-isomp4",
                feature = "symphonia-flac"
            )
        }
    }
}
