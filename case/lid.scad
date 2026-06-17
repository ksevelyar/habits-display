include <mixin.scad>;

module lid() {
  translate([-display_width / 2 - wall, 0, wall]) esp32c3_mini_rails();

  difference() {
    intersection() {
      translate([-wall, wall * 2, wall]) rounded_wedge(
          length=display_width - 0.1,
          width=case_base_width - 0.1,
          height=case_base_height - 0.1,
          r=1.5
        );
      translate([-display_width - wall - 0.1, case_base_width - wall, 0]) cube([display_width - 0.2, wall, case_length - 6]);
    }

    translate([-display_width / 2 - wall, display_length, 5.5]) type_c_cutout();

    translate([-display_width / 2 - wall, display_length - 10, height * 1.65]) rotate([90, 0, 180]) cylinder(10, d=3);
  }
}

lid();
