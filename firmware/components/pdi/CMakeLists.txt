idf_component_register(
    SRCS "bridge.cpp" "vendor/EPD_Driver.cpp"
    INCLUDE_DIRS "." "compat" "vendor"
    REQUIRES driver esp_timer
)
target_compile_features(${COMPONENT_LIB} PRIVATE cxx_std_20)
