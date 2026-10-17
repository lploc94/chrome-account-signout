fn main() {
    let result = (|| {
        let root = chrome_platform::standard_root()?;
        let app = chrome_platform::discover(&root)?;
        let mut chrome = chrome_platform::Chrome::new(root)?;
        let state = chrome.snapshot()?;
        Ok::<_, signout_core::Error>((
            app.executable.is_file(),
            state.main.len(),
            state.relevant.len(),
        ))
    })();
    match result {
        Ok((verified, main, relevant)) => {
            println!("verified={verified} main={main} relevant={relevant}")
        }
        Err(e) => {
            eprintln!("Discovery unavailable: {e}");
            std::process::exit(1)
        }
    }
}
