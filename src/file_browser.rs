use termion::event::Key;

use std::error::Error;
use std::io::Write;

use crate::coordinates::{Coordinates};
use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;
use crate::tabview::Tabbable;

#[derive(PartialEq)]
pub struct FileBrowser {
    pub columns: MillerColumns<ListView<Files>>,
}

impl Tabbable<FileBrowser> for FileBrowser {
    fn new_tab(&self) -> FileBrowser {
        FileBrowser::new().unwrap()
    }

    fn on_next_tab(&mut self) {
        self.update_preview();
    }
}

impl FileBrowser {
    pub fn new() -> Result<FileBrowser, Box<Error>> {
        let cwd = std::env::current_dir().unwrap();
        let coords = Coordinates::new_at(crate::term::xsize(),
                                         crate::term::ysize() - 2,
                                         1,
                                         2);

        let mut miller = MillerColumns::new();
        miller.set_coordinates(&coords);


        let lists: Result<Vec<ListView<Files>>, Box<Error>> = cwd
            .ancestors()
            .map(|path| Ok(ListView::new(Files::new_from_path(path)?)))
            .take(2)
            .collect();
        let mut lists = lists?;
        lists.reverse();

        for widget in lists {
            miller.push_widget(widget);
        }

        let mut file_browser = FileBrowser { columns: miller };


        file_browser.fix_selection();
        file_browser.animate_columns();
        file_browser.update_preview();

        Ok(file_browser)
    }

    pub fn enter_dir(&mut self) {
        let file = self.selected_file();

        match file.read_dir() {
            Ok(files) => {
                std::env::set_current_dir(&file.path).unwrap();
                let view = ListView::new(files);
                self.columns.push_widget(view);
                self.update_preview();
            },
            _ => {
                let status = std::process::Command::new("rifle")
                    .args(file.path.file_name())
                    .status();

                match status {
                    Ok(status) =>
                        self.show_status(&format!("\"{}\" exited with {}",
                                                  "rifle", status)),
                    Err(err) =>
                        self.show_status(&format!("Can't run this \"{}\": {}",
                                                  "rifle", err))

                }
            }
        }
    }

    pub fn go_back(&mut self) {
        if self.columns.get_left_widget().is_none() {
            return;
        }
        let fileview = self.columns.get_main_widget();
        let path = fileview.selected_file().grand_parent().unwrap();
        std::env::set_current_dir(path).unwrap();
        self.columns.pop_widget();

        // Make sure there's a directory on the left unless it's /
        if self.columns.get_left_widget().is_none() {
            let file = self.columns.get_main_widget().clone_selected_file();
            if let Some(grand_parent) = file.grand_parent() {
                let mut left_view = ListView::new(Files::new_from_path(&grand_parent).unwrap());
                left_view.select_file(&file);
                self.columns.prepend_widget(left_view);
            }
        }
        self.fix_selection();
        self.columns.refresh();
    }

    pub fn update_preview(&mut self) {
        if self.columns.get_main_widget().content.len() == 0 { return }
        let file = self.columns.get_main_widget().selected_file().clone();
        let preview = &mut self.columns.preview;
        preview.set_file(&file);
    }

    pub fn fix_selection(&mut self) {
        let cwd = self.cwd();
        self.columns.get_left_widget_mut()
            .map(|w|
                 w.select_file(&cwd));
    }

    pub fn cwd(&self) -> File {
        self.columns.get_main_widget().content.directory.clone()
    }

    pub fn selected_file(&self) -> &File {
        self.main_column().selected_file()
    }

    pub fn main_column(&self) -> &ListView<Files> {
        self.columns.get_main_widget()
    }

    pub fn quit_with_dir(&self) {
        let cwd = self.cwd().path;
        let selected_file = self.selected_file().path.to_string_lossy();

        let mut filepath = dirs_2::home_dir().unwrap();
        filepath.push(".hunter_cwd");

        let output = format!("HUNTER_CWD=\"{}\"\nF=\"{}\"",
                             cwd.to_str().unwrap(),
                             selected_file);

        let mut file = std::fs::File::create(filepath).unwrap();
        file.write(output.as_bytes()).unwrap();
        panic!("Quitting!");
    }

    pub fn animate_columns(&mut self) {
        self.columns.get_left_widget_mut().map(|w| w.animate_slide_up());
        self.columns.get_main_widget_mut().animate_slide_up();
    }

    pub fn turbo_cd(&mut self) {
        let dir = self.minibuffer("cd: ");

        match dir {
            Some(dir) => {
                Files::new_from_path(&std::path::PathBuf::from(&dir)).and_then(|files| {
                    let cwd = files.directory.clone();
                    self.columns.widgets.widgets.clear();
                    self.columns.push_widget(ListView::new(files));

                    std::env::set_current_dir(&cwd.path).unwrap();

                    if let Some(grand_parent) = cwd.path.parent() {
                        let left_view =
                            ListView::new(Files::new_from_path(&grand_parent).unwrap());
                        self.columns.prepend_widget(left_view);
                    }
                    self.fix_selection();
                    self.update_preview();
                    self.refresh();
                    self.columns.refresh();
                    Ok(())
                }).ok();
            } None => {}
        }
    }
}

impl Widget for FileBrowser {
    fn get_coordinates(&self) -> &Coordinates {
        &self.columns.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.columns.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String {
        let xsize = self.get_coordinates().xsize();
        let file = self.selected_file();
        let name = &file.name;

        let color = if file.is_dir() || file.color.is_none() {
            crate::term::highlight_color() } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap()) };

        let path = file.path.parent().unwrap().to_string_lossy().to_string();

        let pretty_path = format!("{}/{}{}", path, &color, name );
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        sized_path
    }
    fn render_footer(&self) -> String {
        let xsize = self.get_coordinates().xsize();
        let ypos = self.get_coordinates().position().y();
        let file = self.selected_file();

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());


        let selection = self.main_column().get_selection();
        let file_count = self.main_column().content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates().ysize();

        format!("{} {}:{} {} {} {}", permissions, user, group, mtime,
                crate::term::goto_xy(count_xpos, count_ypos), file_count)
     }
    fn refresh(&mut self) {
        self.columns.refresh();
    }

    fn get_drawlist(&self) -> String {
        if self.columns.get_left_widget().is_none() {
            self.columns.get_clearlist() + &self.columns.get_drawlist()
        } else {
            self.columns.get_drawlist()
        }
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Char('/') => self.turbo_cd(),
            Key::Char('Q') => self.quit_with_dir(),
            Key::Right | Key::Char('f') => self.enter_dir(),
            Key::Left | Key::Char('b') => self.go_back(),
            _ => self.columns.get_main_widget_mut().on_key(key),
        }
        self.update_preview();
    }
}