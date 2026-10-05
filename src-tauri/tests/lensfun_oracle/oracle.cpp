// Writes expected.json: what the Lensfun C++ library computes for a few
// lenses of the bundled database. The test `lensfun_oracle` in
// src/image_processing.rs compares RapidRoom against it. See README.md.

#include <lensfun.h>

#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

struct Case
{
    const char *label;
    const char *lens_maker;
    const char *lens_model; // canonical name, as in the XML without lang
    float lens_crop;        // selects one entry when a lens has several
    const char *camera_maker; // as the EXIF tag Make
    const char *camera_model; // as the EXIF tag Model
    float focal;
    float aperture; // 0: no vignetting
    float distance;
    int width;
    int height;
};

static const Case CASES[] = {
    // ptlens, Sony FE on full frame, 3:2, 16:9 and portrait
    {"sony-fe-28-70-a7iii-3x2", "Sony", "FE 28-70mm f/3.5-5.6 OSS", 1.0f, "SONY", "ILCE-7M3",
     28.0f, 3.5f, 1000.0f, 6000, 4000},
    {"sony-fe-28-70-a7iii-16x9", "Sony", "FE 28-70mm f/3.5-5.6 OSS", 1.0f, "SONY", "ILCE-7M3",
     28.0f, 3.5f, 1000.0f, 6000, 3376},
    {"sony-fe-28-70-a7iii-portrait", "Sony", "FE 28-70mm f/3.5-5.6 OSS", 1.0f, "SONY", "ILCE-7M3",
     70.0f, 0.0f, 0.0f, 4000, 6000},
    // ptlens, full frame calibration on APS-C
    {"sony-fe-28-70-a6400", "Sony", "FE 28-70mm f/3.5-5.6 OSS", 1.0f, "SONY", "ILCE-6400",
     28.0f, 3.5f, 1000.0f, 6000, 4000},
    // ptlens, Tamron for Sony E
    {"tamron-28-75-a7iii", "Tamron", "E 28-75mm F2.8-2.8", 1.0f, "SONY", "ILCE-7M3",
     28.0f, 2.8f, 1000.0f, 6000, 4000},
    {"tamron-17-28-a6400", "Tamron", "E 17-28mm F2.8-2.8", 1.0f, "SONY", "ILCE-6400",
     17.0f, 2.8f, 1000.0f, 6000, 4000},
    // ptlens, APS-C calibration on APS-C
    {"sony-e-10-18-a6400", "Sony", "E 10-18mm f/4 OSS", 1.534f, "SONY", "ILCE-6400",
     10.0f, 0.0f, 0.0f, 6000, 4000},
    // poly3, Sigma APS-C lens on APS-C
    {"sigma-30-a6400", "Sigma", "E 30mm f/2.8", 1.534f, "SONY", "ILCE-6400",
     30.0f, 2.8f, 1000.0f, 6000, 4000},
    // poly3, Sigma full frame lens on APS-C
    {"sigma-24-art-a6400", "Sigma", "Sigma 24mm f/1.4 DG HSM | [A] Art 015", 1.0f, "SONY",
     "ILCE-6400", 24.0f, 1.4f, 1000.0f, 6000, 4000},
    // poly3, 4:3 calibration on Micro Four Thirds
    {"sigma-19-dn-em10-4x3", "Sigma", "Sigma 19mm f/2.8 DN", 2.0f, "OLYMPUS IMAGING CORP.",
     "E-M10", 19.0f, 2.8f, 1000.0f, 4608, 3456},
    // poly5, 4:3 compact camera
    {"canon-g12-poly5", "Canon", "Canon PowerShot G12 & compatibles (Standard)", 4.63f, "Canon",
     "Canon PowerShot G12", 6.1f, 0.0f, 0.0f, 3648, 2736},
    // Canon EF lens on an EF-M body through the adapter
    {"canon-ef-50-m6ii-distortion", "Canon", "Canon EF 50mm f/1.8 STM", 1.0f, "Canon",
     "Canon EOS M6 Mark II", 50.0f, 0.0f, 0.0f, 6960, 4640},
    {"canon-ef-50-m6ii-vignetting", "Canon", "Canon EF 50mm f/1.8 STM", 1.613f, "Canon",
     "Canon EOS M6 Mark II", 50.0f, 1.8f, 1000.0f, 6960, 4640},
};

static const lfLens *find_lens(lfDatabase *db, const Case &c)
{
    const lfLens *const *lenses = db->GetLenses();
    for (int i = 0; lenses && lenses[i]; i++)
    {
        const lfLens *l = lenses[i];
        if (!strcmp(l->Maker, c.lens_maker) && !strcmp(l->Model, c.lens_model) &&
            std::fabs(l->CropFactor - c.lens_crop) < 1e-3)
            return l;
    }
    return nullptr;
}

static const lfCamera *find_camera(lfDatabase *db, const Case &c)
{
    const lfCamera **cams = db->FindCamerasExt(c.camera_maker, c.camera_model);
    const lfCamera *hit = cams ? cams[0] : nullptr;
    lf_free(cams);
    return hit;
}

// Sample points: towards the corner, the long edge, the short edge and
// along an oblique ray, in pixel coordinates.
static std::vector<std::pair<double, double>> sample_points(int width, int height)
{
    const double cx = (width - 1) / 2.0;
    const double cy = (height - 1) / 2.0;
    const double rays[4][2] = {{cx, cy}, {cx, 0.0}, {0.0, cy}, {cx * 0.4, cy}};
    std::vector<std::pair<double, double>> points;
    for (const auto &ray : rays)
        for (int k = 1; k <= 10; k++)
        {
            const double f = k / 10.0;
            points.push_back({cx + ray[0] * f, cy + ray[1] * f});
        }
    return points;
}

int main(int argc, char **argv)
{
    if (argc < 2)
    {
        fprintf(stderr, "usage: %s <lensfun_db directory>\n", argv[0]);
        return 2;
    }
    lfDatabase *db = new lfDatabase();
    if (db->Load(argv[1]) != LF_NO_ERROR)
    {
        fprintf(stderr, "cannot load %s\n", argv[1]);
        return 1;
    }

    printf("{\n  \"generator\": \"src-tauri/tests/lensfun_oracle/oracle.cpp\",\n");
    printf("  \"lensfun_version\": \"%d.%d.%d.%d\",\n", LF_VERSION_MAJOR, LF_VERSION_MINOR,
           LF_VERSION_MICRO, LF_VERSION_BUGFIX);
    printf("  \"cases\": [\n");

    const size_t n = sizeof(CASES) / sizeof(CASES[0]);
    for (size_t i = 0; i < n; i++)
    {
        const Case &c = CASES[i];
        const lfLens *lens = find_lens(db, c);
        const lfCamera *camera = find_camera(db, c);
        if (!lens || !camera)
        {
            fprintf(stderr, "%s: lens or camera not found\n", c.label);
            return 1;
        }
        const float crop = camera->CropFactor;
        const double cx = (c.width - 1) / 2.0;
        const double cy = (c.height - 1) / 2.0;
        const double half_diagonal = std::hypot(c.width, c.height) / 2.0;
        const auto points = sample_points(c.width, c.height);

        printf("    {\n      \"label\": \"%s\",\n", c.label);
        printf("      \"lens_maker\": \"%s\",\n      \"lens_model\": \"%s\",\n", c.lens_maker,
               c.lens_model);
        printf("      \"lens_crop\": %.9g,\n", c.lens_crop);
        printf("      \"camera_maker\": \"%s\",\n      \"camera_model\": \"%s\",\n",
               c.camera_maker, c.camera_model);
        printf("      \"camera_crop\": %.9g,\n", crop);
        printf("      \"focal\": %.9g,\n      \"aperture\": %.9g,\n      \"distance\": %.9g,\n",
               c.focal, c.aperture, c.distance);
        printf("      \"width\": %d,\n      \"height\": %d,\n", c.width, c.height);

        // Distortion: ratio of the source radius to the output radius.
        printf("      \"distortion\": [");
        lfModifier dist(lens, c.focal, crop, c.width, c.height, LF_PF_F32, false);
        const bool has_distortion = dist.EnableDistortionCorrection() & LF_MODIFY_DISTORTION;
        bool first = true;
        for (const auto &p : points)
        {
            if (!has_distortion)
                break;
            float res[2];
            dist.ApplyGeometryDistortion(p.first, p.second, 1, 1, res);
            const double ru = std::hypot(p.first - cx, p.second - cy);
            const double rd = std::hypot(res[0] - cx, res[1] - cy);
            printf("%s[%.9g, %.9g]", first ? "" : ", ", ru / half_diagonal, rd / ru);
            first = false;
        }
        printf("],\n");

        // Vignetting: the gain that Lensfun applies to a pixel of value 1.
        printf("      \"vignetting\": [");
        first = true;
        if (c.aperture > 0)
        {
            lfModifier vig(lens, c.focal, crop, c.width, c.height, LF_PF_F32, false);
            if (vig.EnableVignettingCorrection(c.aperture, c.distance) & LF_MODIFY_VIGNETTING)
                for (const auto &p : points)
                {
                    float px[3] = {1.0f, 1.0f, 1.0f};
                    vig.ApplyColorModification(px, p.first, p.second, 1, 1,
                                               LF_CR_3(RED, GREEN, BLUE), 0);
                    const double ru = std::hypot(p.first - cx, p.second - cy);
                    printf("%s[%.9g, %.9g]", first ? "" : ", ", ru / half_diagonal, px[0]);
                    first = false;
                }
        }
        printf("]\n    }%s\n", i + 1 < n ? "," : "");
    }
    printf("  ]\n}\n");
    delete db;
    return 0;
}
