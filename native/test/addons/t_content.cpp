#include "wine_test.hpp"
#include <swofty/career_content.hpp>

namespace content = swofty::career::content;

namespace {

bool at_init()
{
    content::Content c{"wine-test", "lng_wine_test_content", "lng_wine_test_content_description", "f1_career_wine",
                       "wt",        "driver_career_wine"};
    swofty::log("[t_content] at init: available %d, add %s", content::available(), swofty::career::name(content::add(c)));
    content::Content dup = c;
    dup.id = "wine-test-2";
    swofty::log("[t_content] at init: duplicate data group %s", swofty::career::name(content::add(dup)));
    content::Content stock = c;
    stock.id = "wine-stock";
    stock.data_group = "f1_career_2026";
    swofty::log("[t_content] at init: stock data group %s", swofty::career::name(content::add(stock)));
    return true;
}

}

SWOFTY_ON_LOAD
{
    content::Info info{};
    swofty::career::Result r = content::info("wine-test", &info);
    std::string group, pressed;
    swofty::career::Result g = swofty::career::data_group(&group);
    swofty::career::Result p = content::pressed(&pressed);
    swofty::log("[t_content] at load: info %s state %u (%s), available %d, data_group %s, pressed %s, is %d",
                swofty::career::name(r), info.state, info.why, content::available(), swofty::career::name(g),
                swofty::career::name(p), swofty::career::is("f1_career_wine"));
}

SWOFTY_ADDON(at_init)
