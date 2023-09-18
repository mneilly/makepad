use {
    std::{
        sync::{
            Arc,
            Mutex,
        },
        io,
        io::prelude::*,
        io::BufReader,
        path::Path,
        io::Write,
        fs,
        process::Command,
    },
    crate::{
        makepad_live_id::*,
        makepad_math::*,
        makepad_error_log::*,
        makepad_micro_serde::*,
        makepad_live_compiler::LiveFileChange,
        event::Event,
        window::CxWindowPool,
        event::WindowGeom,
        texture::Texture,
        live_traits::LiveNew,
        thread::Signal,
        os::{
            apple_sys::*,
            metal_xpc::{
                xpc_service_proxy,
                xpc_service_proxy_poll_run_loop,
                fetch_xpc_service_texture,
            },
            metal::{MetalCx, DrawPassMode},
            cx_stdin::{HostToStdin, StdinToHost},
        },
        pass::{CxPassParent, PassClearColor, CxPassColorTexture},
        cx_api::CxOsOp,
        cx::Cx,
    }
};

impl Cx {
    
    pub (crate) fn stdin_send_draw_complete(present_index: &Arc<Mutex<usize>>){

        // get the current present index
        let mut index = present_index.lock().unwrap();

        // send message
        let _ = io::stdout().write_all(StdinToHost::DrawCompleteAndFlip(*index).to_json().as_bytes());

        // flip swapchain
        *index = 1 - *index;
    }
    
    pub (crate) fn stdin_handle_repaint(&mut self, metal_cx: &mut MetalCx) {
        let mut passes_todo = Vec::new();
        self.compute_pass_repaint_order(&mut passes_todo);
        self.repaint_id += 1;
        for pass_id in &passes_todo {
            match self.passes[*pass_id].parent.clone() {
                CxPassParent::Window(_) => {
                    // render to swapchain
                    self.draw_pass(*pass_id, metal_cx, DrawPassMode::StdinMain);

                    // and then wait for GPU, which calls stdin_send_draw_complete when its done
                }
                CxPassParent::Pass(_) => {
                    //let dpi_factor = self.get_delegated_dpi_factor(parent_pass_id);
                    self.draw_pass(*pass_id, metal_cx, DrawPassMode::Texture);
                },
                CxPassParent::None => {
                    self.draw_pass(*pass_id, metal_cx, DrawPassMode::Texture);
                }
            }
        }
    }
    
    pub fn stdin_event_loop(&mut self, metal_cx: &mut MetalCx) {
        let _ = io::stdout().write_all(StdinToHost::ReadyToStart.to_json().as_bytes());
        let service_proxy = xpc_service_proxy();
        let mut reader = BufReader::new(std::io::stdin());
        let mut window_size = None;
        
        self.call_event_handler(&Event::Construct);
        
        loop {
            let mut line = String::new();
            if let Ok(len) = reader.read_line(&mut line) {
                if len == 0 {
                    break
                }
                // alright lets put the line in a json parser
                let parsed: Result<HostToStdin, DeJsonErr> = DeJson::deserialize_json(&line);
                
                match parsed {
                    Ok(msg) => match msg {
                        HostToStdin::ReloadFile {file, contents} => {
                            // alright lets reload this file in our DSL system
                            let _ = self.live_file_change_sender.send(vec![LiveFileChange{
                                file_name: file,
                                content: contents
                            }]);
                        }
                        HostToStdin::KeyDown(e) => {
                            self.call_event_handler(&Event::KeyDown(e));
                        }
                        HostToStdin::KeyUp(e) => {
                            self.call_event_handler(&Event::KeyUp(e));
                        }
                        HostToStdin::MouseDown(e) => {
                            self.fingers.process_tap_count(
                                dvec2(e.x, e.y),
                                e.time
                            );
                            self.fingers.mouse_down(e.button);
                            
                            self.call_event_handler(&Event::MouseDown(e.into()));
                        }
                        HostToStdin::MouseMove(e) => {
                            self.call_event_handler(&Event::MouseMove(e.into()));
                            self.fingers.cycle_hover_area(live_id!(mouse).into());
                            self.fingers.switch_captures();
                        }
                        HostToStdin::MouseUp(e) => {
                            let button = e.button;
                            self.call_event_handler(&Event::MouseUp(e.into()));
                            self.fingers.mouse_up(button);
                            self.fingers.cycle_hover_area(live_id!(mouse).into());
                        }
                        HostToStdin::Scroll(e) => {
                            self.call_event_handler(&Event::Scroll(e.into()))
                        }
                        HostToStdin::WindowSize(ws) => {

                            log!("got WindowSize {},{}, fetching XPC textures",ws.width,ws.height);

                            // start fetching new texture objects from XPC
                            fetch_xpc_service_texture(service_proxy.as_id(),0,0,Box::new({
                                let maybe_new_handle = Arc::clone(&self.os.maybe_new_handles[0]);
                                move |objcid,_| {
                                    log!("fetched XPC texture 0: {:?}",objcid.as_id());
                                    *maybe_new_handle.lock().unwrap() = Some(objcid);
                                }
                            }));
                            fetch_xpc_service_texture(service_proxy.as_id(),1,0,Box::new({
                                let maybe_new_handle = Arc::clone(&self.os.maybe_new_handles[1]);
                                move |objcid,_| {
                                    log!("fetched XPC texture 1: {:?}",objcid.as_id());
                                    *maybe_new_handle.lock().unwrap() = Some(objcid);
                                }
                            }));

                            // The textures will be placed in cx.os.maybe_new_handles[] by the XPC service and HostToStdin::Tick checks if they have arrived to process them further

                            // and window size might have changed
                            if window_size != Some(ws) {
                                window_size = Some(ws);
                                self.redraw_all();
                                
                                let window = &mut self.windows[CxWindowPool::id_zero()];
                                window.window_geom = WindowGeom {
                                    dpi_factor: ws.dpi_factor,
                                    inner_size: dvec2(ws.width, ws.height),
                                    ..Default::default()
                                };
                                self.stdin_handle_platform_ops(metal_cx);
                            }
                        }
                        HostToStdin::Tick {frame: _, buffer_id: _, time} => if let Some(ws) = window_size {
                            
                            // check if new handles have already arrived from XPC
                            let maybe_handle0 = self.os.maybe_new_handles[0].lock().unwrap().clone();
                            let maybe_handle1 = self.os.maybe_new_handles[1].lock().unwrap().clone();
                            if maybe_handle0.is_some() && maybe_handle1.is_some() {

                                log!("both texture handles arrived from XPC, create new textures accordingly");

                                // make sure the corresponding Metal textures exist
                                let new_textures = [Texture::new(self),Texture::new(self),];
                                let cxtexture = &mut self.textures[new_textures[0].texture_id()];
                                cxtexture.os.update_from_shared_handle(
                                    metal_cx,
                                    maybe_handle0.as_ref().unwrap().as_id(),
                                    ws.width as u64,
                                    ws.height as u64,
                                );
                                let cxtexture = &mut self.textures[new_textures[1].texture_id()];
                                cxtexture.os.update_from_shared_handle(
                                    metal_cx,
                                    maybe_handle1.as_ref().unwrap().as_id(),
                                    ws.width as u64,
                                    ws.height as u64,
                                );

                                // update the swapchain resources
                                self.os.swapchain = Some(new_textures);

                                // and reset present index
                                *self.os.present_index.lock().unwrap() = 0;

                                // clear the handles, until the next resize comes in
                                *self.os.maybe_new_handles[0].lock().unwrap() = None;
                                *self.os.maybe_new_handles[1].lock().unwrap() = None;
                            }

                            // check signals
                            if Signal::check_and_clear_ui_signal() {
                                self.handle_media_signals();
                                self.call_event_handler(&Event::Signal);
                            }
                            if self.handle_live_edit() {
                                self.call_event_handler(&Event::LiveEdit);
                                self.redraw_all();
                            }
                            self.handle_networking_events();
                            
                            // alright a tick.
                            // we should now run all the stuff.
                            if self.new_next_frames.len() != 0 {
                                self.call_next_frame_event(time);
                            }
                            
                            if self.need_redrawing() {
                                self.call_draw_event();
                                self.mtl_compile_shaders(metal_cx);
                            }
                            
                            self.stdin_handle_repaint(metal_cx);
                        }
                    }
                    Err(err) => { // we should output a log string
                        error!("Cant parse stdin-JSON {} {:?}", line, err);
                    }
                }
            }
            // we should poll our runloop
            self.stdin_handle_platform_ops(metal_cx);
            xpc_service_proxy_poll_run_loop();
        }
    }
    
    pub(crate)fn start_xpc_service(&mut self){
        
        pub fn mkdir(path: &Path) -> Result<(), String> {
            match fs::create_dir_all(path) { 
                Err(e) => {
                    Err(format!("mkdir {:?} failed {:?}", path, e))
                },
                Ok(()) => Ok(())
            }
        }
        
        pub fn shell(cwd: &Path, cmd: &str, args: &[&str]) -> Result<(), String> {
            let mut cmd_build = Command::new(cmd);
            
            cmd_build.args(args)
                .current_dir(cwd);
            
            let mut child = cmd_build.spawn().map_err( | e | format!("Error starting {} in dir {:?} - {:?}", cmd, cwd, e)) ?;
            
            let r = child.wait().map_err( | e | format!("Process {} in dir {:?} returned error {:?} ", cmd, cwd, e)) ?;
            if !r.success() {
                return Err(format!("Process {} in dir {:?} returned error exit code ", cmd, cwd));
            }
            Ok(())
        }
        
        pub fn write_text(path: &Path, data:&str) -> Result<(), String> {
            mkdir(path.parent().unwrap()) ?;
            match fs::File::create(path) { 
                Err(e) => {
                    Err(format!("file create {:?} failed {:?}", path, e))
                },
                Ok(mut f) =>{
                    f.write_all(data.as_bytes())
                        .map_err( | _e | format!("Cant write file {:?}", path))
                }
            }
        }
        
        pub fn get_exe_path()->String{
            let buf = [0u8;1024];
            let mut len = 1024u32;
            unsafe{_NSGetExecutablePath(buf.as_ptr() as *mut _, &mut len)};
            let end = buf.iter().position(|v| *v == 0).unwrap();
            std::str::from_utf8(&buf[0..end]).unwrap().to_string()
        }
        
        let exe_path = get_exe_path();
        
        let plist_body = format!(r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
            <plist version="1.0">
            <dict>
                <key>Label</key>
                <string>dev.makepad.metalxpc</string>
                <key>Program</key>
                <string>{exe_path}</string>
                <key>ProgramArguments</key>
                <array>
                    <string>{exe_path}</string>
                    <string>--metal-xpc</string>
                </array>
                <key>MachServices</key>
                <dict>
                    <key>dev.makepad.metalxpc</key>
                    <true/>
                </dict>
            </dict>
            </plist>
            "#,
        );
        // lets write our service
        let home = std::env::var("HOME").unwrap();
        let plist_path = format!("{}/Library/LaunchAgents/dev.makepad.xpc.plist", home);
        let cwd = std::env::current_dir().unwrap();
        if let Ok(old) = fs::read_to_string(Path::new(&plist_path)){
            if old == plist_body{
                return
            }
        }
        shell(&cwd, "launchctl",&["unload",&plist_path]).unwrap();
        write_text(Path::new(&plist_path), &plist_body).unwrap();
        shell(&cwd, "launchctl",&["load",&plist_path]).unwrap();
    }
    
    
    fn stdin_handle_platform_ops(&mut self, _metal_cx: &MetalCx) {
        while let Some(op) = self.platform_ops.pop() {
            match op {
                CxOsOp::CreateWindow(_window_id) => {
                    let window = &mut self.windows[CxWindowPool::id_zero()];
                    window.is_created = true;
                    // lets set up our render pass target
                    let pass = &mut self.passes[window.main_pass_id.unwrap()];
                    if let Some(swapchain) = self.os.swapchain.as_ref() {
                        let present_index = *self.os.present_index.lock().unwrap();
                        pass.color_textures = vec![CxPassColorTexture {
                            //clear_color: PassClearColor::ClearWith(vec4(1.0, 1.0, 0.0, 1.0)),
                            clear_color: PassClearColor::ClearWith(pass.clear_color),
                            texture_id: swapchain[present_index].texture_id()
                        }];
                    }
                },
                CxOsOp::SetCursor(cursor) => {
                    let _ = io::stdout().write_all(StdinToHost::SetCursor(cursor).to_json().as_bytes());
                },
                _ => ()
                /*
                CxOsOp::CloseWindow(_window_id) => {},
                CxOsOp::MinimizeWindow(_window_id) => {},
                CxOsOp::MaximizeWindow(_window_id) => {},
                CxOsOp::RestoreWindow(_window_id) => {},
                CxOsOp::FullscreenWindow(_window_id) => {},
                CxOsOp::NormalizeWindow(_window_id) => {}
                CxOsOp::SetTopmost(_window_id, _is_topmost) => {}
                CxOsOp::XrStartPresenting(_) => {},
                CxOsOp::XrStopPresenting(_) => {},
                CxOsOp::ShowTextIME(_area, _pos) => {},
                CxOsOp::HideTextIME => {},
                CxOsOp::SetCursor(_cursor) => {},
                CxOsOp::StartTimer {timer_id, interval, repeats} => {},
                CxOsOp::StopTimer(timer_id) => {},
                CxOsOp::StartDragging(dragged_item) => {}
                CxOsOp::UpdateMenu(menu) => {}*/
            }
        }
    }
    
}
