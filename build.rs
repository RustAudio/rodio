use cfg_aliases::cfg_aliases;

fn main() {
    // Add alias to see if any symphonia features are enabled
    // This prevents having to copy/paste this large cfg check each time
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
