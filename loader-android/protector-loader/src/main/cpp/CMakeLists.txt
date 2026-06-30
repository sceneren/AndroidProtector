cmake_minimum_required(VERSION 3.22.1)
project(protector_vm)

add_library(protector_vm SHARED protector_vm.cpp)

find_library(log-lib log)
target_link_libraries(protector_vm ${log-lib})

