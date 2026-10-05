// Renders what a lens records of a straight grid: Lensfun in reverse mode
// adds the distortion and the vignetting of a profile to an ideal chart.
// Output: 16-bit PGM on stdout. See README.md.

#include <lensfun.h>

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

int main(int argc, char **argv)
{
    if (argc < 10)
    {
        fprintf(stderr, "usage: %s <db> <lens maker> <lens model> <lens crop> <camera crop> "
                        "<focal> <aperture> <width> <height>\n", argv[0]);
        return 2;
    }
    lfDatabase db;
    if (db.Load(argv[1]) != LF_NO_ERROR)
        return 1;
    const float lens_crop = atof(argv[4]), crop = atof(argv[5]), focal = atof(argv[6]);
    const float aperture = atof(argv[7]);
    const int w = atoi(argv[8]), h = atoi(argv[9]);

    const lfLens *lens = nullptr;
    for (const lfLens *const *l = db.GetLenses(); l && *l; l++)
        if (!strcmp((*l)->Maker, argv[2]) && !strcmp((*l)->Model, argv[3]) &&
            std::fabs((*l)->CropFactor - lens_crop) < 1e-3)
            lens = *l;
    if (!lens)
        return 1;

    lfModifier mod(lens, focal, crop, w, h, LF_PF_F32, true);
    mod.EnableDistortionCorrection();
    lfModifier vig(lens, focal, crop, w, h, LF_PF_F32, true);
    vig.EnableVignettingCorrection(aperture, 1000.0f);

    const double cell = h / 10.0;
    std::vector<float> coords(2 * w);
    printf("P5\n%d %d\n65535\n", w, h);
    for (int y = 0; y < h; y++)
    {
        mod.ApplyGeometryDistortion(0, y, w, 1, coords.data());
        for (int x = 0; x < w; x++)
        {
            const double u = coords[2 * x] - (w - 1) / 2.0, v = coords[2 * x + 1] - (h - 1) / 2.0;
            const double gu = std::fabs(std::remainder(u, cell)), gv = std::fabs(std::remainder(v, cell));
            float px[3];
            px[0] = px[1] = px[2] = (gu < 1.5 || gv < 1.5) ? 0.05f : 0.6f;
            vig.ApplyColorModification(px, x, y, 1, 1, LF_CR_3(RED, GREEN, BLUE), 0);
            const int value = (int)std::lround(std::min(1.0f, std::max(0.0f, px[0])) * 65535.0f);
            putchar(value >> 8);
            putchar(value & 0xff);
        }
    }
    return 0;
}
