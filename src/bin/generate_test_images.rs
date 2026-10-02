use image::{ImageBuffer, ImageFormat, Rgb};
use std::fs;
use std::path::Path;

fn draw_filled_rect(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    color: Rgb<u8>,
) {
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            img.put_pixel(x, y, color);
        }
    }
}

fn main() {
    let out_dir = Path::new("test_assets");
    fs::create_dir_all(out_dir).expect("Failed to create test_assets dir");

    // 1. Car A Front: Silver car body, black wheels, yellow headlights
    let mut car_front = ImageBuffer::from_pixel(640, 480, Rgb([240, 240, 245])); // background
    draw_filled_rect(&mut car_front, 120, 200, 400, 160, Rgb([180, 185, 190])); // silver body
    draw_filled_rect(&mut car_front, 180, 120, 280, 80, Rgb([100, 150, 200])); // windshield
    draw_filled_rect(&mut car_front, 160, 340, 60, 60, Rgb([30, 30, 30])); // left wheel
    draw_filled_rect(&mut car_front, 420, 340, 60, 60, Rgb([30, 30, 30])); // right wheel
    draw_filled_rect(&mut car_front, 130, 240, 40, 30, Rgb([255, 230, 50])); // headlight L
    draw_filled_rect(&mut car_front, 470, 240, 40, 30, Rgb([255, 230, 50])); // headlight R
    draw_filled_rect(&mut car_front, 260, 300, 120, 35, Rgb([255, 255, 255])); // license plate
    car_front
        .save_with_format(out_dir.join("car_front.jpg"), ImageFormat::Jpeg)
        .expect("Failed to save car_front.jpg");

    // 2. Car A Rear: Matching silver body, black wheels, red taillights
    let mut car_rear = ImageBuffer::from_pixel(640, 480, Rgb([240, 240, 245]));
    draw_filled_rect(&mut car_rear, 120, 200, 400, 160, Rgb([180, 185, 190])); // silver body
    draw_filled_rect(&mut car_rear, 180, 120, 280, 80, Rgb([80, 120, 160])); // rear window
    draw_filled_rect(&mut car_rear, 160, 340, 60, 60, Rgb([30, 30, 30])); // left wheel
    draw_filled_rect(&mut car_rear, 420, 340, 60, 60, Rgb([30, 30, 30])); // right wheel
    draw_filled_rect(&mut car_rear, 130, 240, 40, 30, Rgb([220, 30, 30])); // taillight L
    draw_filled_rect(&mut car_rear, 470, 240, 40, 30, Rgb([220, 30, 30])); // taillight R
    draw_filled_rect(&mut car_rear, 260, 300, 120, 35, Rgb([255, 255, 255])); // matching plate
    car_rear
        .save_with_format(out_dir.join("car_rear.jpg"), ImageFormat::Jpeg)
        .expect("Failed to save car_rear.jpg");

    // 3. Animal (Cat silhouette): Orange body, ears
    let mut animal = ImageBuffer::from_pixel(640, 480, Rgb([220, 240, 220]));
    draw_filled_rect(&mut animal, 240, 180, 160, 160, Rgb([230, 130, 40])); // orange body
    draw_filled_rect(&mut animal, 240, 130, 40, 50, Rgb([230, 130, 40])); // left ear
    draw_filled_rect(&mut animal, 360, 130, 40, 50, Rgb([230, 130, 40])); // right ear
    draw_filled_rect(&mut animal, 270, 210, 20, 20, Rgb([40, 180, 80])); // eye L
    draw_filled_rect(&mut animal, 350, 210, 20, 20, Rgb([40, 180, 80])); // eye R
    animal
        .save_with_format(out_dir.join("animal_cat.jpg"), ImageFormat::Jpeg)
        .expect("Failed to save animal_cat.jpg");

    println!(
        "Generated test_assets/car_front.jpg, test_assets/car_rear.jpg, and test_assets/animal_cat.jpg"
    );
}
