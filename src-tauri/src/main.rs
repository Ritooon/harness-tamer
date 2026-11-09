// Pas de console supplémentaire sous Windows en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    harnessforge_lib::run()
}
