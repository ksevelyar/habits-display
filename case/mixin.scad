wall = 2;
display_length = 47;
display_width = 31.5;
esp32c3_width = 18.5;
esp32c3_length = 22;
height = 24;

case_width = display_width + wall * 2;
case_length = display_length + wall * 2;
case_base_width = 41.2;
case_base_height = 39.55;

$fn = 128;

module rail_side(half_width, length, y) {
  translate([-half_width - 2, y, 0]) cube([2, length, 6]);
  hull() {
    translate([-half_width - 0.15, y, 2.5]) rotate([0, -40, 0]) cube([1.5, length, 1]);
    translate([-half_width - 0.5, y, 3.4]) cube([1.5, length, 1]);
  }
  translate([-half_width - 0.35, y, 5]) rotate([0, -40, 0]) cube([1.3, length, 1]);
}

module rail(half_width, length, y) {
  rail_side(half_width, length, y);
  mirror([1, 0, 0]) rail_side(half_width, length, y);
}

module esp32c3_mini_rails() {
  rail(esp32c3_width / 2, esp32c3_length, 19);
}

module type_c_cutout() {
  rotate([90, 0, 0])
    hull() {
      translate([-3.1, 0, 0]) cylinder(h=10, d=3.7);
      translate([3.1, 0, 0]) cylinder(h=10, d=3.7);
    }
}

module leg(leg_height) {
  difference() {
    cylinder(h=leg_height, d=5);
    translate([0, 0, -0.1]) cylinder(h=leg_height + 1, d=3);
  }
}

module rounded_box(size = [10, 10, 5], r) {
  translate([size[0] / 2, size[1] / 2, 0])
    linear_extrude(height = size[2])
      offset(r = r)
        square([size[0] - 2 * r, size[1] - 2 * r], center = true);
}

module rounded_wedge(length, width, height, r = 1.5) {
  translate([0, 0, r * 2]) hull() {
      translate([0, r, -r])
        rotate([0, 90, 180])
          cylinder(h=length, d=r * 2);

      translate([0, width - r, -r])
        rotate([0, 90, 180])
          cylinder(h=length, d=r * 2);

      translate([0, width - r, height + r])
        rotate([0, 90, 180])
          cylinder(h=length, d=r * 2);
    }
}
