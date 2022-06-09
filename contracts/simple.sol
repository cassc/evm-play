pragma solidity ^0.7.0;
contract Branching {
    uint public count = 10;
    function run() public returns (uint){
        count = count +1 ;
        return count;
    }
}
