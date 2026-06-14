include <mixin.scad>;

$fn = 128;

module lid() {
  translate([-34, 0, 0]) {
    translate([-31.2 / 2 - wall, -wall, wall]) esp32c3_mini_rails();

    translate([-display_width / 2 - wall - 12.5, display_length - esp32c3_length - wall * 3, wall + 7]) rotate([90, 0, 180]) leg(esp32c3_length);
    translate([-display_width / 2 - wall + 12.5, display_length - esp32c3_length - wall * 3, wall + 7]) rotate([90, 0, 180]) leg(esp32c3_length);

    difference() {
      intersection() {
        translate([-wall, wall * 2, wall]) rounded_wedge(
            length=display_width,
            width=41.1,
            height=39.45,
            r=1.5
          );
        translate([-display_width - wall, 41.2 - wall, 0]) cube([display_width, wall, case_length]);
      }

      translate([-display_width / 2 - wall, display_length, 8.5]) type_c_cutout();
      translate([-display_width / 2 - wall, display_length - 10, height * 1.7]) rotate([90, 0, 180]) cylinder(10, d=3);;
    }
  }
}

lid();
